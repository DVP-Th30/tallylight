//! Liaisons de la bibliothèque d'écran pour les applications (Kotlin, Swift), générées par UniFFI.
//!
//! Une application crée un [`Tallylight`], appaire une ou plusieurs machines avec
//! [`Tallylight::pair`], puis reçoit l'état de chacune par son [`Listener`]. La bibliothèque garde
//! l'identité de l'écran et les machines appairées dans le dossier fourni, et se reconnecte seule.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tallylight_client::{Error, Screen};
use tokio::{runtime::Runtime, sync::OnceCell, task::JoinHandle};

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
    /// Outil en cours d'exécution, ou outil pour lequel une autorisation est demandée.
    pub tool: Option<String>,
    /// Ce que fait l'outil, en une ligne : description d'une commande, nom d'un fichier, question posée.
    pub detail: Option<String>,
    /// Début du dernier message de l'agent, quand il a fini son tour.
    pub last_message: Option<String>,
    /// Adresse qui ouvre la session dans l'application de l'agent, si elle en a une.
    pub url: Option<String>,
    /// Dernier changement, en secondes depuis 1970.
    pub updated_at: u64,
}

/// État du lien avec une machine suivie.
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

/// Une machine suivie : son lien et ses sessions (les dernières connues quand elle est injoignable).
#[derive(Debug, Clone, uniffi::Record)]
pub struct Machine {
    /// Identité stable de la machine ; clé pour la reconnaître et pour [`Tallylight::forget`].
    pub id: String,
    /// Nom de la machine ; vide tant qu'elle n'a jamais répondu.
    pub name: String,
    pub link: Link,
    pub sessions: Vec<Session>,
}

/// Ce que l'application reçoit. Les appels arrivent sur un fil de la bibliothèque.
#[uniffi::export(with_foreign)]
pub trait Listener: Send + Sync {
    /// Une machine a changé : lien ou sessions. Appelé aussi une fois par machine au démarrage.
    fn on_machine(&self, machine: Machine);
    /// La machine a été retirée par [`Tallylight::forget`].
    fn on_machine_removed(&self, id: String);
}

/// Une machine appairée, telle qu'elle est conservée.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Saved {
    id: String,
    ticket: String,
    /// Dernier nom connu, pour l'afficher avant que la machine réponde.
    #[serde(default)]
    name: String,
}

#[derive(Default)]
struct Running {
    listener: Option<Arc<dyn Listener>>,
    tasks: HashMap<String, JoinHandle<()>>,
    /// L'écran (une seule identité, un seul point réseau) partagé par toutes les machines suivies.
    screen: Arc<OnceCell<Arc<Screen>>>,
}

#[derive(uniffi::Object)]
pub struct Tallylight {
    runtime: Runtime,
    data_dir: PathBuf,
    name: String,
    running: Mutex<Running>,
}

#[uniffi::export]
impl Tallylight {
    /// `data_dir` : dossier privé de l'application ; `name` : nom de l'écran, montré sur les machines.
    #[uniffi::constructor]
    pub fn new(data_dir: String, name: String) -> Arc<Self> {
        let data_dir = PathBuf::from(data_dir);
        let _ = std::fs::create_dir_all(&data_dir);
        migrate(&data_dir);
        Arc::new(Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("démarrage du moteur asynchrone"),
            data_dir,
            name,
            running: Mutex::new(Running::default()),
        })
    }

    /// Nombre de machines appairées depuis cet écran.
    pub fn machines(&self) -> u32 {
        load(&self.data_dir).len() as u32
    }

    /// Commence à suivre toutes les machines appairées.
    ///
    /// Garder les connexions ouvertes consomme de la batterie : appeler [`Tallylight::stop`] quand
    /// l'écran n'est pas visible.
    pub fn start(&self, listener: Arc<dyn Listener>) {
        self.stop();
        let mut running = self.running.lock().unwrap();
        running.listener = Some(listener);
        for machine in load(&self.data_dir) {
            self.follow(&mut running, machine.id.clone(), machine.ticket.clone());
        }
    }

    /// Appaire une machine à partir de la chaîne de `tallylight pair` et commence à la suivre.
    /// Sans effet avant [`Tallylight::start`]. Retourne faux si la chaîne est illisible.
    pub fn pair(&self, pairing: String) -> bool {
        let Ok(id) = tallylight_client::machine_id(&pairing) else {
            return false;
        };
        let mut running = self.running.lock().unwrap();
        if running.listener.is_none() {
            return false;
        }
        self.follow(&mut running, id, pairing);
        true
    }

    /// Cesse de suivre une machine et l'oublie.
    pub fn forget(&self, id: String) {
        let mut running = self.running.lock().unwrap();
        if let Some(task) = running.tasks.remove(&id) {
            task.abort();
        }
        let mut machines = load(&self.data_dir);
        machines.retain(|machine| machine.id != id);
        save(&self.data_dir, &machines);
        if let Some(listener) = &running.listener {
            listener.on_machine_removed(id);
        }
    }

    /// Ferme les connexions ; [`Tallylight::start`] les rouvre.
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        for (_, task) in running.tasks.drain() {
            task.abort();
        }
        running.listener = None;
        let screen = std::mem::take(&mut running.screen);
        self.runtime.spawn(async move {
            if let Some(screen) = screen.get() {
                screen.close().await;
            }
        });
    }
}

impl Tallylight {
    /// Lance (ou relance) le suivi d'une machine. `target` : chaîne d'appairage ou ticket conservé.
    fn follow(&self, running: &mut Running, id: String, target: String) {
        let Some(listener) = running.listener.clone() else {
            return;
        };
        if let Some(task) = running.tasks.remove(&id) {
            task.abort();
        }
        let follower = Follower {
            data_dir: self.data_dir.clone(),
            screen_name: self.name.clone(),
            screen: running.screen.clone(),
            listener,
            machine: Machine {
                name: load(&self.data_dir)
                    .into_iter()
                    .find(|machine| machine.id == id)
                    .map(|machine| machine.name)
                    .unwrap_or_default(),
                id: id.clone(),
                link: Link::Connecting,
                sessions: Vec::new(),
            },
        };
        running
            .tasks
            .insert(id, self.runtime.spawn(follower.run(target)));
    }
}

/// Le suivi d'une machine : garde son dernier état connu et prévient l'application à chaque changement.
struct Follower {
    data_dir: PathBuf,
    screen_name: String,
    screen: Arc<OnceCell<Arc<Screen>>>,
    listener: Arc<dyn Listener>,
    machine: Machine,
}

impl Follower {
    fn link(&mut self, link: Link) {
        if self.machine.link != link {
            self.machine.link = link;
            self.listener.on_machine(self.machine.clone());
        }
    }

    /// Suit la machine jusqu'à l'arrêt, en se reconnectant après chaque coupure.
    async fn run(mut self, mut target: String) {
        self.listener.on_machine(self.machine.clone());
        let mut wait = Duration::from_secs(2);
        loop {
            self.link(Link::Connecting);
            let screen = self
                .screen
                .get_or_try_init(|| async {
                    Screen::new(screen_key(&self.data_dir), self.screen_name.clone())
                        .await
                        .map(Arc::new)
                })
                .await
                .cloned();
            let connected = match screen {
                Ok(screen) => screen.watch(&target).await,
                Err(error) => Err(error),
            };
            match connected {
                Ok(mut watch) => {
                    // La chaîne d'appairage ne sert qu'une fois : on garde le ticket seul
                    target = watch.ticket().to_string();
                    self.remember(&target);
                    wait = Duration::from_secs(2);
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
                        // Tant qu'aucun chemin n'est établi, l'écran reste sur « connexion »
                        let link = match watch.relayed() {
                            Some(true) => Link::Relayed,
                            Some(false) => Link::Direct,
                            None => Link::Connecting,
                        };
                        match snapshot {
                            Some(snapshot) => {
                                if self.machine.name != snapshot.machine {
                                    self.machine.name = snapshot.machine.clone();
                                    self.remember(&target);
                                }
                                self.machine.link = link;
                                self.machine.sessions = convert(snapshot.sessions);
                                self.listener.on_machine(self.machine.clone());
                            }
                            None => self.link(link),
                        }
                    }
                }
                Err(Error::NotPaired) | Err(Error::BadPairing) => {
                    self.link(Link::NotPaired);
                    return;
                }
                Err(Error::Connection(_)) => {}
            }
            self.link(Link::Offline);
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(Duration::from_secs(60));
        }
    }

    /// Conserve la machine (ticket et nom) pour les prochains lancements.
    fn remember(&self, ticket: &str) {
        let mut machines = load(&self.data_dir);
        machines.retain(|machine| machine.id != self.machine.id);
        machines.push(Saved {
            id: self.machine.id.clone(),
            ticket: ticket.to_string(),
            name: self.machine.name.clone(),
        });
        machines.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        save(&self.data_dir, &machines);
    }
}

fn load(data_dir: &Path) -> Vec<Saved> {
    std::fs::read(data_dir.join("machines.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(data_dir: &Path, machines: &[Saved]) {
    if let Ok(bytes) = serde_json::to_vec_pretty(machines) {
        let _ = std::fs::write(data_dir.join("machines.json"), bytes);
    }
}

/// Reprend la machine unique des premières versions (fichier `machine`, un ticket seul).
fn migrate(data_dir: &Path) {
    let old = data_dir.join("machine");
    let Ok(ticket) = std::fs::read_to_string(&old) else {
        return;
    };
    if let Ok(id) = tallylight_client::machine_id(&ticket) {
        let mut machines = load(data_dir);
        if !machines.iter().any(|machine| machine.id == id) {
            machines.push(Saved {
                id,
                ticket: ticket.trim().to_string(),
                name: String::new(),
            });
            save(data_dir, &machines);
        }
    }
    let _ = std::fs::remove_file(old);
}

/// Identité de l'écran, créée au premier lancement et conservée dans le dossier de l'application.
fn screen_key(data_dir: &Path) -> [u8; 32] {
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

fn convert(sessions: Vec<tallylight_client::Session>) -> Vec<Session> {
    sessions
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
            detail: session.detail,
            last_message: session.last_message,
            url: session.url,
            updated_at: session.updated_at,
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_single_machine_of_early_versions_is_kept() {
        let dir = std::env::temp_dir().join(format!("tallylight-ffi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("machine"), "ticket illisible").unwrap();
        migrate(&dir);
        // Un ancien ticket illisible est abandonné, sans erreur
        assert!(load(&dir).is_empty());
        assert!(!dir.join("machine").exists());

        save(
            &dir,
            &[Saved {
                id: "abc".into(),
                ticket: "t".into(),
                name: "serveur".into(),
            }],
        );
        assert_eq!(load(&dir)[0].name, "serveur");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
