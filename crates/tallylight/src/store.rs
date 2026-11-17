//! État des sessions de la machine, et sa diffusion aux écrans connectés.

use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tallylight_core::{
    Session, Snapshot,
    claude::{Change, HookEvent, change},
};
use tokio::sync::watch;

pub struct Store {
    machine: String,
    sessions: Mutex<BTreeMap<String, Session>>,
    snapshot: watch::Sender<Snapshot>,
}

impl Store {
    pub fn new(machine: String) -> Self {
        let (snapshot, _) = watch::channel(Snapshot {
            machine: machine.clone(),
            sessions: Vec::new(),
        });
        Self {
            machine,
            sessions: Mutex::new(BTreeMap::new()),
            snapshot,
        }
    }

    /// Applique un événement de Claude Code ; les écrans ne sont prévenus que si quelque chose change.
    pub fn apply_claude(&self, event: &HookEvent) {
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.get(&event.session_id).cloned();
        match change(event) {
            Change::Ignored => return,
            Change::Ended => {
                sessions.remove(&event.session_id);
            }
            update => {
                let session = sessions
                    .entry(event.session_id.clone())
                    .or_insert_with(|| Session {
                        id: event.session_id.clone(),
                        agent: "claude-code".to_string(),
                        cwd: String::new(),
                        state: None,
                        tool: None,
                        updated_at: now(),
                    });
                if !event.cwd.is_empty() {
                    session.cwd = event.cwd.clone();
                }
                if let Change::Set { state, tool } = update {
                    if session.state != Some(state) || session.tool != tool {
                        session.updated_at = now();
                    }
                    session.state = Some(state);
                    session.tool = tool;
                }
            }
        }
        if sessions.get(&event.session_id) != before.as_ref() {
            self.snapshot.send_replace(Snapshot {
                machine: self.machine.clone(),
                sessions: sessions.values().cloned().collect(),
            });
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.subscribe()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tallylight_core::State;

    fn event(json: &str) -> HookEvent {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn sessions_follow_their_events() {
        let store = Store::new("m".into());
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
}
