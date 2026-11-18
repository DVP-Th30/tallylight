//! État des sessions de la machine, sa conservation et sa diffusion aux écrans connectés.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Mutex,
};

use tallylight_core::{
    Session, Snapshot, State,
    claude::{Change, HookEvent, change, last_message},
};
use tokio::sync::watch;

use crate::{claude_local::LocalSession, now, write_private};

/// Délai laissé au hook `Stop` avant de conclure qu'un tour a été interrompu.
const INTERRUPT_GRACE: u64 = 3;

/// Âge au-delà duquel une session sans fiche locale ni événement est oubliée.
const FORGET_AFTER: u64 = 12 * 3600;

/// Délai laissé à une session qui s'est seulement annoncée pour montrer sa fiche locale ou commencer
/// un tour ; passé ce délai, sans l'un ni l'autre, c'est une session fantôme.
const GHOST_GRACE: u64 = 60;

pub struct Store {
    machine: String,
    /// Fichier où les sessions sont conservées d'un lancement à l'autre.
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
    snapshot: watch::Sender<Snapshot>,
}

#[derive(Default)]
struct Inner {
    sessions: BTreeMap<String, Session>,
    /// Date du dernier événement reçu de chaque session, qu'il ait changé l'état ou non.
    last_event: HashMap<String, u64>,
}

impl Store {
    /// `path` : fichier des sessions conservées ; elles sont relues au démarrage.
    pub fn new(machine: String, path: Option<PathBuf>) -> Self {
        let sessions: BTreeMap<String, Session> = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<Session>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|session| (session.id.clone(), session))
            .collect();
        let (snapshot, _) = watch::channel(Snapshot {
            machine: machine.clone(),
            sessions: sessions.values().cloned().collect(),
        });
        Self {
            machine,
            path,
            inner: Mutex::new(Inner {
                sessions,
                last_event: HashMap::new(),
            }),
            snapshot,
        }
    }

    /// Applique un événement de Claude Code ; les écrans ne sont prévenus que si quelque chose change.
    pub fn apply_claude(&self, event: &HookEvent) {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.sessions.clone();
        match change(event) {
            Change::Ignored => return,
            Change::Ended => {
                inner.sessions.remove(&event.session_id);
                inner.last_event.remove(&event.session_id);
            }
            update => {
                inner.last_event.insert(event.session_id.clone(), now());
                let session = inner
                    .sessions
                    .entry(event.session_id.clone())
                    .or_insert_with(|| blank(&event.session_id));
                if !event.cwd.is_empty() {
                    session.cwd = event.cwd.clone();
                }
                // Le dernier message vaut jusqu'au tour suivant
                if let Some(message) = last_message(event) {
                    session.last_message = Some(message);
                } else if event.hook_event_name == "UserPromptSubmit" {
                    session.last_message = None;
                }
                match update {
                    Change::Set {
                        state,
                        tool,
                        detail,
                    } => {
                        if session.state != Some(state) || session.tool != tool {
                            session.updated_at = now();
                        }
                        session.state = Some(state);
                        session.tool = tool;
                        session.detail = detail;
                    }
                    // Attente signalée sans précision : on garde l'outil et le détail déjà connus
                    Change::Waiting if session.state != Some(State::InputRequired) => {
                        session.updated_at = now();
                        session.state = Some(State::InputRequired);
                    }
                    _ => {}
                }
            }
        }
        self.publish(&inner, &before);
    }

    /// Rapproche les sessions connues de ce que Claude Code écrit sur la machine :
    /// - une session vivante encore inconnue apparaît (ouverte avant le démon, ou avant ses hooks) ;
    /// - une session dont le processus est mort disparaît (fermée sans événement de fin) ;
    /// - un tour « au travail » dont le processus est revenu au repos sans hook de fin a été interrompu.
    pub fn reconcile(&self, local: &HashMap<String, LocalSession>, now: u64) {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.sessions.clone();
        let Inner {
            sessions,
            last_event,
        } = &mut *inner;

        sessions.retain(|id, session| match local.get(id) {
            Some(card) => card.alive,
            // Annoncée par un hook de démarrage, jamais vue au travail et sans processus : un
            // lancement sans écran d'un autre outil, ou l'identifiant provisoire d'une reprise
            None if session.state.is_none() => now.saturating_sub(session.updated_at) < GHOST_GRACE,
            None => now.saturating_sub(session.updated_at) < FORGET_AFTER,
        });
        last_event.retain(|id, _| sessions.contains_key(id));

        for (id, card) in local.iter().filter(|(_, card)| card.alive) {
            let session = sessions.entry(id.clone()).or_insert_with(|| {
                let mut session = blank(id);
                session.cwd = card.cwd.clone();
                session.updated_at = now;
                session
            });
            session.title = card.title.clone();
            session.url = card.url.clone();
            let quiet_since = last_event.get(id).copied().unwrap_or(session.updated_at);
            let interrupted = session.state == Some(State::Working)
                && card.status.as_deref() == Some("idle")
                && card.status_at >= quiet_since
                && now.saturating_sub(card.status_at) >= INTERRUPT_GRACE;
            if interrupted {
                session.state = Some(State::Canceled);
                session.tool = None;
                session.detail = None;
                session.updated_at = card.status_at;
            }
        }
        self.publish(&inner, &before);
    }

    /// Prévient les écrans et enregistre, seulement si les sessions ont changé.
    fn publish(&self, inner: &Inner, before: &BTreeMap<String, Session>) {
        if &inner.sessions == before {
            return;
        }
        let sessions: Vec<Session> = inner.sessions.values().cloned().collect();
        if let Some(path) = &self.path
            && let Ok(bytes) = serde_json::to_vec(&sessions)
        {
            let _ = write_private(path, &bytes);
        }
        self.snapshot.send_replace(Snapshot {
            machine: self.machine.clone(),
            sessions,
        });
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.subscribe()
    }
}

fn blank(id: &str) -> Session {
    Session {
        id: id.to_string(),
        agent: "claude-code".to_string(),
        cwd: String::new(),
        title: None,
        state: None,
        tool: None,
        detail: None,
        last_message: None,
        url: None,
        updated_at: now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(json: &str) -> HookEvent {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn sessions_follow_their_events() {
        let store = Store::new("m".into(), None);
        let mut updates = store.subscribe();
        store.apply_claude(&event(
            r#"{"session_id":"a","hook_event_name":"UserPromptSubmit","cwd":"/p/demo"}"#,
        ));
        assert!(updates.has_changed().unwrap());
        let snapshot = updates.borrow_and_update().clone();
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].state, Some(State::Working));
        assert_eq!(snapshot.sessions[0].project(), "demo");

        // Un événement qui ne change rien ne réveille pas les écrans
        store.apply_claude(&event(
            r#"{"session_id":"a","hook_event_name":"PostToolUse","cwd":"/p/demo"}"#,
        ));
        assert!(!updates.has_changed().unwrap());

        store.apply_claude(&event(r#"{"session_id":"a","hook_event_name":"Stop"}"#));
        assert_eq!(store.snapshot().sessions[0].state, Some(State::Completed));
        assert_eq!(store.snapshot().sessions[0].cwd, "/p/demo");

        store.apply_claude(&event(
            r#"{"session_id":"a","hook_event_name":"SessionEnd"}"#,
        ));
        assert!(store.snapshot().sessions.is_empty());
    }

    fn local(alive: bool, status: &str, status_at: u64) -> LocalSession {
        LocalSession {
            alive,
            cwd: "/p/demo".into(),
            title: Some("Refonte".into()),
            status: Some(status.into()),
            status_at,
            url: None,
        }
    }

    #[test]
    fn local_cards_reveal_sessions_and_interruptions() {
        let store = Store::new("m".into(), None);
        let t = now();

        // Session ouverte avant le démon : elle apparaît, sans état
        store.reconcile(
            &HashMap::from([("a".to_string(), local(true, "idle", t))]),
            t,
        );
        let session = store.snapshot().sessions[0].clone();
        assert_eq!((session.state, session.label()), (None, "Refonte"));

        // Un tour commence, puis le processus revient au repos sans hook de fin : interrompu
        store.apply_claude(&event(
            r#"{"session_id":"a","hook_event_name":"UserPromptSubmit"}"#,
        ));
        store.reconcile(
            &HashMap::from([("a".to_string(), local(true, "busy", t))]),
            t + 10,
        );
        assert_eq!(store.snapshot().sessions[0].state, Some(State::Working));
        store.reconcile(
            &HashMap::from([("a".to_string(), local(true, "idle", t + 20))]),
            t + 21,
        );
        assert_eq!(store.snapshot().sessions[0].state, Some(State::Working));
        store.reconcile(
            &HashMap::from([("a".to_string(), local(true, "idle", t + 20))]),
            t + 24,
        );
        assert_eq!(store.snapshot().sessions[0].state, Some(State::Canceled));

        // Processus mort : la session disparaît
        store.reconcile(
            &HashMap::from([("a".to_string(), local(false, "idle", t + 20))]),
            t + 30,
        );
        assert!(store.snapshot().sessions.is_empty());
    }

    #[test]
    fn announced_sessions_without_a_process_are_ghosts() {
        let store = Store::new("m".into(), None);
        let t = now();
        store.apply_claude(&event(
            r#"{"session_id":"ghost","hook_event_name":"SessionStart","cwd":"/p/cache"}"#,
        ));
        store.apply_claude(&event(
            r#"{"session_id":"done","hook_event_name":"UserPromptSubmit","cwd":"/p/demo"}"#,
        ));
        store.apply_claude(&event(r#"{"session_id":"done","hook_event_name":"Stop"}"#));
        let ids = |store: &Store| -> Vec<String> {
            let sessions = store.snapshot().sessions;
            sessions.into_iter().map(|session| session.id).collect()
        };

        // Le temps que la fiche locale apparaisse, la session annoncée reste
        store.reconcile(&HashMap::new(), t + 30);
        assert_eq!(ids(&store), ["done", "ghost"]);
        // Ni fiche ni tour : fantôme. La session qui a travaillé reste, même sans fiche
        store.reconcile(&HashMap::new(), t + 90);
        assert_eq!(ids(&store), ["done"]);
    }

    #[test]
    fn sessions_survive_a_restart() {
        let path =
            std::env::temp_dir().join(format!("tallylight-store-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::new("m".into(), Some(path.clone()));
        store.apply_claude(&event(
            r#"{"session_id":"a","hook_event_name":"PermissionRequest","cwd":"/p/demo","tool_name":"Bash"}"#,
        ));
        let reloaded = Store::new("m".into(), Some(path.clone()));
        assert_eq!(
            reloaded.snapshot().sessions[0].state,
            Some(State::InputRequired)
        );
        let _ = std::fs::remove_file(&path);
    }
}
