//! Liaisons de la bibliothèque d'écran pour les applications (Kotlin, Swift), générées par UniFFI.
//!
//! Une application crée un [`Tallylight`], lui donne une chaîne d'appairage une fois, puis reçoit
//! les instantanés par son [`Listener`]. La bibliothèque garde l'identité de l'écran et le ticket de
//! la machine dans le dossier fourni, et se reconnecte seule.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use tallylight_client::{Error, Screen};
use tokio::{runtime::Runtime, task::JoinHandle};

uniffi::setup_scaffolding!();

/// Une session d'agent, prête à afficher.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Session {
    pub id: String,
    /// Agent qui tient la session (« claude-code »).
    pub agent: String,
    /// Nom court : dernier élément du dossier de travail.
    pub project: String,
    pub cwd: String,
    /// Nom donné à la session, s'il y en a un ; sinon afficher `project`.
    pub title: Option<String>,
    /// État dans le vocabulaire A2A (`working`, `input-required`, `completed`, `failed`,
    /// `canceled`) ; vide tant qu'aucun tour n'a commencé.
    pub state: String,
    /// Outil en cours d'exécution, quand l'agent travaille.
    pub tool: Option<String>,
    /// Dernier changement, en secondes depuis 1970.
    pub updated_at: u64,
}

/// Toutes les sessions d'une machine à un instant donné.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Snapshot {
    pub machine: String,
    pub sessions: Vec<Session>,
}

/// État du lien avec la machine suivie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Link {
    Connecting,
    /// Connecté en direct.
    Direct,
    /// Connecté par un relais.
    Relayed,
    /// La machine ne connaît pas cet écran : il faut une nouvelle chaîne d'appairage.
    NotPaired,
    /// Machine injoignable ; une nouvelle tentative suivra.
    Offline,
}

/// Ce que l'application reçoit. Les appels arrivent sur un fil de la bibliothèque.
#[uniffi::export(with_foreign)]
pub trait Listener: Send + Sync {
    fn on_snapshot(&self, snapshot: Snapshot);
    fn on_link(&self, link: Link);
}

#[derive(uniffi::Object)]
pub struct Tallylight {
    runtime: Runtime,
    data_dir: PathBuf,
    name: String,
    task: Mutex<Option<JoinHandle<()>>>,
}

#[uniffi::export]
impl Tallylight {
    /// `data_dir` : dossier privé de l'application ; `name` : nom de l'écran, montré sur la machine.
    #[uniffi::constructor]
    pub fn new(data_dir: String, name: String) -> Arc<Self> {
        Arc::new(Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("démarrage du moteur asynchrone"),
            data_dir: PathBuf::from(data_dir),
            name,
            task: Mutex::new(None),
        })
    }

    /// Vrai si une machine a déjà été appairée depuis cet écran.
    pub fn paired(&self) -> bool {
        self.data_dir.join("machine").exists()
    }

    /// Commence à suivre la machine. `pairing` : chaîne de `tallylight pair` pour un premier
    /// appairage, ou vide pour retrouver la machine déjà appairée.
    ///
    /// Garder la connexion ouverte consomme de la batterie : appeler [`Tallylight::stop`] quand
    /// l'écran n'est pas visible.
    pub fn start(&self, pairing: String, listener: Arc<dyn Listener>) {
        self.stop();
        let data_dir = self.data_dir.clone();
        let name = self.name.clone();
        let task = self
            .runtime
            .spawn(async move { follow(data_dir, name, pairing, listener).await });
        *self.task.lock().unwrap() = Some(task);
    }

    /// Ferme la connexion ; [`Tallylight::start`] la rouvre.
    pub fn stop(&self) {
        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
        }
    }
}

/// Suit la machine jusqu'à l'arrêt, en se reconnectant après chaque coupure.
async fn follow(data_dir: PathBuf, name: String, pairing: String, listener: Arc<dyn Listener>) {
    let _ = std::fs::create_dir_all(&data_dir);
    let key = screen_key(&data_dir);
    let saved = data_dir.join("machine");
    // La chaîne d'appairage ne sert qu'une fois : ensuite on revient par le ticket conservé
    let mut target = pairing.trim().to_string();
    let mut wait = Duration::from_secs(2);
    loop {
        if target.is_empty() {
            target = std::fs::read_to_string(&saved).unwrap_or_default();
        }
        if target.trim().is_empty() {
            listener.on_link(Link::NotPaired);
            return;
        }
        listener.on_link(Link::Connecting);
        let screen = match Screen::new(key, name.clone()).await {
            Ok(screen) => screen,
            Err(_) => {
                listener.on_link(Link::Offline);
                tokio::time::sleep(wait).await;
                continue;
            }
        };
        match screen.watch(&target).await {
            Ok(mut watch) => {
                let _ = std::fs::write(&saved, watch.ticket());
                target.clear();
                wait = Duration::from_secs(2);
                let mut link = None;
                // Le lien commence souvent par un relais puis passe en direct : on le réévalue
                // régulièrement, pas seulement à l'arrivée d'un instantané
                let mut check = tokio::time::interval(Duration::from_secs(3));
                loop {
                    let snapshot = tokio::select! {
                        next = watch.next() => match next {
                            Ok(Some(snapshot)) => Some(snapshot),
                            _ => break,
                        },
                        _ = check.tick() => None,
                    };
                    let now = if watch.relayed() {
                        Link::Relayed
                    } else {
                        Link::Direct
                    };
                    if link != Some(now) {
                        link = Some(now);
                        listener.on_link(now);
                    }
                    if let Some(snapshot) = snapshot {
                        listener.on_snapshot(convert(snapshot));
                    }
                }
            }
            Err(Error::NotPaired) | Err(Error::BadPairing) => {
                listener.on_link(Link::NotPaired);
                screen.close().await;
                return;
            }
            Err(Error::Connection(_)) => {}
        }
        screen.close().await;
        listener.on_link(Link::Offline);
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(Duration::from_secs(60));
    }
}

/// Identité de l'écran, créée au premier lancement et conservée dans le dossier de l'application.
fn screen_key(data_dir: &std::path::Path) -> [u8; 32] {
    let path = data_dir.join("screen-key");
    if let Ok(bytes) = std::fs::read(&path)
        && let Ok(key) = <[u8; 32]>::try_from(bytes)
    {
        return key;
    }
    let key = tallylight_client::generate_key();
    let _ = std::fs::write(&path, key);
    key
}

fn convert(snapshot: tallylight_client::Snapshot) -> Snapshot {
    Snapshot {
        machine: snapshot.machine,
        sessions: snapshot
            .sessions
            .into_iter()
            .map(|session| Session {
                project: session.project().to_string(),
                state: session
                    .state
                    .map(|state| state.as_a2a().to_string())
                    .unwrap_or_default(),
                id: session.id,
                agent: session.agent,
                cwd: session.cwd,
                title: session.title,
                tool: session.tool,
                updated_at: session.updated_at,
            })
            .collect(),
    }
}

/// Android : donne à la bibliothèque réseau l'accès au système (lecture des serveurs DNS).
/// À appeler une fois, avant [`Tallylight::start`], par `dev.tallylight.Android.init(context)`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_tallylight_Android_init<'local>(
    env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    context: jni::objects::JObject<'local>,
) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let (Ok(vm), Ok(context)) = (env.get_java_vm(), env.new_global_ref(context)) else {
            return;
        };
        let context_pointer = context.as_obj().as_raw();
        // Le contexte doit rester valide jusqu'à la fin du processus
        std::mem::forget(context);
        unsafe {
            iroh::dns::install_android_jni_context(
                vm.get_java_vm_pointer().cast(),
                context_pointer.cast(),
            );
        }
    });
}
