//! Premier écran : l'état des sessions dans le terminal, redessiné à chaque changement.
//!
//! L'écran suit cette machine (si son démon répond) et toutes celles déjà appairées depuis ce
//! terminal (fichier `machines`), chacune dans sa tâche, avec reconnexion après une coupure.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use tallylight_client::{Relays, Screen, Session, Snapshot, State, Watch, machine_id};
use tokio::sync::mpsc;

use crate::{data_dir, identity, local, machine_name, now, write_private};

/// État du lien avec une machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Connecting,
    Direct,
    Relayed,
    Offline,
    NotPaired,
}

/// Ce que l'écran sait d'une machine.
#[derive(Debug, Clone)]
struct View {
    /// Nom annoncé par la machine ; début de son identité tant qu'elle ne s'est pas présentée.
    name: String,
    link: Link,
    /// Dernier instantané reçu sur le lien en cours ; effacé quand le lien tombe.
    snapshot: Option<Snapshot>,
}

/// Temps laissé à une tentative de connexion avant d'afficher la machine hors ligne.
const CONNECT_PATIENCE: Duration = Duration::from_secs(10);

/// Nouvelle d'une tâche de suivi : identité de la machine, lien, et instantané s'il en arrive un.
type Event = (String, Link, Option<Snapshot>);

/// `pairing` : chaîne donnée par `tallylight pair` (« ticket#code »), ou ticket seul d'une machine
/// déjà appairée. Sans argument, on suit cette machine si son démon répond, plus toutes celles
/// déjà appairées depuis ce terminal.
pub async fn run(pairing: Option<&str>, relays: &Relays) -> Result<()> {
    // L'écran garde son identité : une fois appairé, il est reconnu sans code
    let screen = Arc::new(
        Screen::with_relays(
            identity("screen-key")?.to_bytes(),
            format!("terminal@{}", machine_name()),
            relays,
        )
        .await?,
    );
    let (events, mut received) = mpsc::unbounded_channel::<Event>();
    let mut views: BTreeMap<String, View> = BTreeMap::new();

    // Une chaîne donnée à la main est vérifiée tout de suite : un code faux ou périmé doit se voir
    if let Some(pairing) = pairing {
        let id = machine_id(pairing)?;
        let watch = match screen.watch(pairing).await {
            Err(tallylight_client::Error::NotPaired) => bail!(
                "cet écran n'est pas appairé avec la machine : lancer `tallylight pair` sur elle et utiliser la chaîne affichée"
            ),
            other => other?,
        };
        remember(watch.ticket())?;
        let target = watch.ticket().to_string();
        views.insert(id.clone(), unnamed(&id));
        tokio::spawn(follow(
            screen.clone(),
            id,
            target,
            Some(watch),
            events.clone(),
        ));
    }

    let mut targets = known()?;
    // Cette machine, si son démon tourne : il fournit lui-même l'appairage
    let local = local::pairing().await;
    if let Ok(pairing) = &local {
        targets.insert(0, pairing.clone());
    }
    for target in targets {
        let Ok(id) = machine_id(&target) else {
            continue;
        };
        if views.contains_key(&id) {
            continue;
        }
        views.insert(id.clone(), unnamed(&id));
        tokio::spawn(follow(screen.clone(), id, target, None, events.clone()));
    }
    if views.is_empty() {
        // Ni démon local, ni machine connue : l'erreur du démon dit quoi faire
        local?;
        bail!("aucune machine à suivre : lancer `tallylight pair` sur une machine");
    }
    drop(events);

    // Hors d'un terminal (sortie redirigée), on n'efface pas l'écran : un bloc par changement
    let redraw = std::io::IsTerminal::is_terminal(&std::io::stdout());
    while let Some((id, link, snapshot)) = received.recv().await {
        let Some(view) = views.get_mut(&id) else {
            continue;
        };
        view.link = link;
        match snapshot {
            Some(snapshot) => {
                view.name = snapshot.machine.clone();
                view.snapshot = Some(snapshot);
            }
            None if !matches!(link, Link::Direct | Link::Relayed) => view.snapshot = None,
            None => {}
        }
        if redraw {
            print!("\x1b[2J\x1b[H");
        }
        print!("{}", render_all(views.values(), now()));
    }
    screen.close().await;
    Ok(())
}

fn unnamed(id: &str) -> View {
    View {
        name: id.chars().take(10).collect(),
        link: Link::Connecting,
        snapshot: None,
    }
}

/// Suit une machine jusqu'à la fin du programme, en se reconnectant après chaque coupure (attente
/// de 2 à 60 s). S'arrête si la machine ne reconnaît plus l'écran.
async fn follow(
    screen: Arc<Screen>,
    id: String,
    mut target: String,
    mut first: Option<Watch>,
    events: mpsc::UnboundedSender<Event>,
) {
    let send = |link: Link, snapshot: Option<Snapshot>| events.send((id.clone(), link, snapshot));
    let mut wait = Duration::from_secs(2);
    loop {
        let connected = match first.take() {
            Some(watch) => Ok(watch),
            None => {
                // Après une coupure, la machine reste « hors ligne » jusqu'à ce qu'elle réponde
                if wait == Duration::from_secs(2) {
                    let _ = send(Link::Connecting, None);
                }
                // Machine éteinte : la tentative peut durer longtemps, mieux vaut dire « hors ligne »
                match tokio::time::timeout(CONNECT_PATIENCE, screen.watch(&target)).await {
                    Ok(connected) => connected,
                    Err(_) => Err(tallylight_client::Error::Connection("délai dépassé".into())),
                }
            }
        };
        match connected {
            Ok(mut watch) => {
                // La chaîne d'appairage ne sert qu'une fois : on garde le ticket seul
                target = watch.ticket().to_string();
                let _ = remember(&target);
                wait = Duration::from_secs(2);
                // Le lien passe souvent du relais au direct : réévalué régulièrement
                let mut check = tokio::time::interval(Duration::from_secs(3));
                let mut shown = None;
                loop {
                    let snapshot = tokio::select! {
                        next = watch.next() => match next {
                            Ok(Some(snapshot)) => Some(snapshot),
                            _ => break,
                        },
                        _ = check.tick() => None,
                    };
                    let link = match watch.relayed() {
                        Some(true) => Link::Relayed,
                        Some(false) => Link::Direct,
                        None => Link::Connecting,
                    };
                    if snapshot.is_some() || shown != Some(link) {
                        shown = Some(link);
                        if send(link, snapshot).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(tallylight_client::Error::NotPaired | tallylight_client::Error::BadPairing) => {
                let _ = send(Link::NotPaired, None);
                return;
            }
            Err(tallylight_client::Error::Connection(_)) => {}
        }
        if send(Link::Offline, None).is_err() {
            return;
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(Duration::from_secs(60));
    }
}

/// Toutes les machines, dans l'ordre de leur identité, séparées par une ligne vide.
fn render_all<'a>(views: impl Iterator<Item = &'a View>, now: u64) -> String {
    let blocks: Vec<String> = views
        .map(|view| match (&view.snapshot, view.link) {
            (Some(snapshot), Link::Direct | Link::Relayed) => {
                let block = render(snapshot, now);
                if view.link == Link::Relayed {
                    // Le lien passe par un relais : utile pour comprendre une latence
                    block.replacen('\n', " · relais\n", 1)
                } else {
                    block
                }
            }
            (_, Link::NotPaired) => format!(
                "{} · non appairé : `tallylight pair` sur elle, puis `tallylight watch '<chaîne>'`\n",
                view.name
            ),
            (_, Link::Offline) => format!("{} · hors ligne, nouvel essai bientôt\n", view.name),
            _ => format!("{} · connexion…\n", view.name),
        })
        .collect();
    blocks.join("\n")
}

/// Une ligne par session, celles qui attendent l'utilisateur en premier.
fn render(snapshot: &Snapshot, now: u64) -> String {
    let mut sessions: Vec<&Session> = snapshot.sessions.iter().collect();
    sessions.sort_by_key(|session| (rank(session.state), session.label().to_string()));
    let mut out = format!("{} · {} session(s)\n", snapshot.machine, sessions.len());
    for session in sessions {
        let (mark, label) = match session.state {
            Some(State::InputRequired) => ("!", "attend une réponse"),
            Some(State::Working) => ("●", "au travail"),
            Some(State::Completed) => ("✓", "fini"),
            Some(State::Failed) => ("✗", "erreur"),
            Some(State::Canceled) => ("–", "interrompu"),
            None => ("·", "ouverte"),
        };
        // « (Bash : Compile le projet) », ou l'outil seul, ou rien
        let tool = match (session.tool.as_deref(), session.detail.as_deref()) {
            (Some(tool), Some(detail)) => format!(" ({tool} : {detail})"),
            (Some(tool), None) => format!(" ({tool})"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "  {mark} {:<24} {label}{tool} · {}\n",
            session.label(),
            ago(now.saturating_sub(session.updated_at)),
        ));
    }
    out
}

fn rank(state: Option<State>) -> u8 {
    match state {
        Some(State::InputRequired) => 0,
        Some(State::Failed) => 1,
        Some(State::Working) => 2,
        Some(State::Completed) | Some(State::Canceled) => 3,
        None => 4,
    }
}

fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min", seconds / 60),
        _ => format!("{} h", seconds / 3600),
    }
}

/// Retient le ticket d'une machine appairée (un par ligne), pour la retrouver plus tard.
fn remember(ticket: &str) -> Result<()> {
    let path = data_dir()?.join("machines");
    let known = std::fs::read_to_string(&path).unwrap_or_default();
    if known.lines().any(|line| line == ticket) {
        return Ok(());
    }
    write_private(&path, format!("{known}{ticket}\n").as_bytes())
}

/// Tickets des machines déjà appairées depuis ce terminal.
fn known() -> Result<Vec<String>> {
    let known = std::fs::read_to_string(data_dir()?.join("machines")).unwrap_or_default();
    Ok(known
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(cwd: &str, state: Option<State>, tool: Option<&str>, updated_at: u64) -> Session {
        Session {
            id: cwd.into(),
            agent: "claude-code".into(),
            cwd: cwd.into(),
            title: None,
            state,
            tool: tool.map(Into::into),
            detail: None,
            last_message: None,
            url: None,
            updated_at,
        }
    }

    #[test]
    fn sessions_waiting_for_the_user_come_first() {
        let snapshot = Snapshot {
            machine: "serveurdvp".into(),
            sessions: vec![
                session("/p/alpha", Some(State::Working), Some("Bash"), 995),
                session("/p/beta", Some(State::InputRequired), None, 880),
                session("/p/gamma", Some(State::Completed), None, 0),
            ],
        };
        let lines: Vec<String> = render(&snapshot, 1000).lines().map(String::from).collect();
        assert_eq!(lines[0], "serveurdvp · 3 session(s)");
        assert!(lines[1].contains("beta") && lines[1].ends_with("attend une réponse · 2 min"));
        assert!(lines[2].contains("alpha") && lines[2].ends_with("au travail (Bash) · 5 s"));
        assert!(lines[3].contains("gamma") && lines[3].ends_with("fini · 16 min"));
    }

    #[test]
    fn every_machine_shows_its_link() {
        let snapshot = |machine: &str| Snapshot {
            machine: machine.into(),
            sessions: vec![session("/p/alpha", Some(State::Working), None, 1000)],
        };
        let views = [
            View {
                name: "mac".into(),
                link: Link::Relayed,
                snapshot: Some(snapshot("mac")),
            },
            View {
                name: "serveurdvp".into(),
                link: Link::Direct,
                snapshot: Some(snapshot("serveurdvp")),
            },
            View {
                name: "portable".into(),
                link: Link::Offline,
                snapshot: None,
            },
            View {
                name: "ab12cd34ef".into(),
                link: Link::NotPaired,
                snapshot: None,
            },
        ];
        let text = render_all(views.iter(), 1000);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "mac · 1 session(s) · relais");
        assert_eq!(lines[3], "serveurdvp · 1 session(s)");
        assert_eq!(lines[6], "portable · hors ligne, nouvel essai bientôt");
        assert!(lines[8].starts_with("ab12cd34ef · non appairé"));
    }
}
