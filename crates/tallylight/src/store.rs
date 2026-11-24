//! État des sessions de la machine, sa conservation et sa diffusion aux écrans connectés.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Mutex,
};

use tallylight_core::{
    Session, Snapshot, State,
    claude::{self, Change, HookEvent, background_tasks, last_message},
    codex,
};
use tokio::sync::watch;

use crate::{claude_local::LocalSession, now, write_private};

/// Nom de chaque agent dans le champ `agent` d'une session.
const CLAUDE_CODE: &str = "claude-code";
const CODEX: &str = "codex";

/// Délai laissé au hook `Stop` avant de conclure qu'un tour a été interrompu.
const INTERRUPT_GRACE: u64 = 3;

/// Âge au-delà duquel une session sans fiche locale ni événement est oubliée.
const FORGET_AFTER: u64 = 12 * 3600;

/// Délai laissé à une session qui s'est seulement annoncée pour montrer sa fiche locale ou commencer
/// un tour ; passé ce délai, sans l'un ni l'autre, c'est une session fantôme.
const GHOST_GRACE: u64 = 60;

pub struct Store {
    /// Nom de la machine, changé par `tallylight name` sans redémarrer.
    machine: Mutex<String>,
    /// Fichier où les sessions sont conservées d'un lancement à l'autre.
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
    snapshot: watch::Sender<Snapshot>,
    /// Lien donné aux sessions de Claude Code qui n'en ont pas : celui de la session hébergée
    /// en ligne dans laquelle tourne le démon, voir [`hosted_session_url`].
    fallback_url: Option<String>,
}

/// Lien vers la session hébergée (« Claude Code on the web ») dont cet environnement fait partie,
/// tiré de ses variables : `CLAUDE_CODE_REMOTE_SESSION_ID` vaut `cse_…` et le lien remplace ce
/// préfixe par `session_` (https://code.claude.com/docs/en/cloud-environments, lu le 2026-11-23).
pub fn hosted_session_url(remote: Option<&str>, session: Option<&str>) -> Option<String> {
    if remote != Some("true") {
        return None;
    }
    let id = session?.strip_prefix("cse_")?;
    let plain = !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    plain.then(|| format!("https://claude.ai/code/session_{id}"))
}

#[derive(Default)]
struct Inner {
    sessions: BTreeMap<String, Session>,
    /// Date du dernier événement reçu de chaque session, qu'il ait changé l'état ou non.
    last_event: HashMap<String, u64>,
    /// Machines invitées, connectées ou parties depuis peu, par numéro de connexion.
    guests: BTreeMap<u64, Guest>,
    next_guest: u64,
}

/// Une machine invitée : ses sessions s'ajoutent à celles de la machine (D21). Partie, elle reste
/// le temps de [`GUEST_KEPT`] avec ses dernières sessions connues (D22).
struct Guest {
    /// Invitation qui l'a fait entrer.
    invite: String,
    /// Identité réseau de la machine : celle qui revient reprend sa place et son nom.
    machine: String,
    /// Date de son départ ; `None` tant qu'elle est connectée.
    left_at: Option<u64>,
    /// Nom sous lequel ses sessions sont diffusées, imposé par la machine d'accueil.
    host: String,
    sessions: Vec<Session>,
}

/// Machines invitées par invitation, connectées ou gardées après leur départ.
const GUESTS_PER_INVITE: usize = 20;

/// Durée pendant laquelle les dernières sessions connues d'une invitée partie restent affichées.
/// Une machine hébergée est mise en pause dès que sa session est au repos : c'est justement là
/// que l'état « fini » ou « attend une réponse » doit rester visible.
const GUEST_KEPT: u64 = 24 * 3600;

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
            machine: Mutex::new(machine),
            path,
            inner: Mutex::new(Inner {
                sessions,
                last_event: HashMap::new(),
                guests: BTreeMap::new(),
                next_guest: 0,
            }),
            snapshot,
            fallback_url: hosted_session_url(
                std::env::var("CLAUDE_CODE_REMOTE").ok().as_deref(),
                std::env::var("CLAUDE_CODE_REMOTE_SESSION_ID")
                    .ok()
                    .as_deref(),
            ),
        }
    }

    /// Applique un événement de Claude Code ; les écrans ne sont prévenus que si quelque chose change.
    pub fn apply_claude(&self, event: &HookEvent) {
        // Lu hors du verrou : la transcription est un fichier, parfois gros
        let model = event
            .transcript_path
            .as_deref()
            .and_then(|path| crate::transcript::last_model(std::path::Path::new(path)));
        self.apply(CLAUDE_CODE, event, claude::change(event), model);
    }

    /// Applique un événement de Codex. Le modèle est dans l'événement : sa transcription, d'un
    /// autre format que celle de Claude Code, n'est pas lue.
    pub fn apply_codex(&self, event: &HookEvent) {
        let model = event.model.clone().filter(|model| !model.is_empty());
        self.apply(CODEX, event, codex::change(event), model);
    }

    /// Applique à la session de l'événement le changement que son agent en a tiré.
    fn apply(&self, agent: &str, event: &HookEvent, change: Change, model: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.sessions.clone();
        match change {
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
                    .or_insert_with(|| blank(&event.session_id, agent));
                if !event.cwd.is_empty() {
                    session.cwd = event.cwd.clone();
                }
                if model.is_some() {
                    session.model = model;
                }
                // Le dernier message vaut jusqu'au tour suivant
                if let Some(message) = last_message(event) {
                    session.last_message = Some(message);
                } else if event.hook_event_name == "UserPromptSubmit" {
                    session.last_message = None;
                }
                if session.url.is_none() && agent == CLAUDE_CODE {
                    session.url = self.fallback_url.clone();
                }
                if let Some(count) = background_tasks(event) {
                    session.background_tasks = count;
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
            ..
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
                let mut session = blank(id, CLAUDE_CODE);
                session.cwd = card.cwd.clone();
                session.updated_at = now;
                session
            });
            session.title = card.title.clone();
            session.url = card.url.clone().or_else(|| self.fallback_url.clone());
            if card.model.is_some() {
                session.model = card.model.clone();
            }
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

    /// Prévient les écrans et enregistre, seulement si les sessions de la machine ont changé.
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
        self.broadcast(inner);
    }

    /// Diffuse les sessions de la machine suivies de celles de ses invitées, si l'ensemble a changé.
    /// Les sessions invitées ne sont pas enregistrées : elles reviennent avec leur machine.
    fn broadcast(&self, inner: &Inner) {
        let mut sessions: Vec<Session> = inner.sessions.values().cloned().collect();
        let mut seen: std::collections::HashSet<&str> =
            inner.sessions.keys().map(String::as_str).collect();
        // Un identifiant ne paraît qu'une fois : celui de la machine l'emporte, puis celui d'une
        // invitée connectée sur celui d'une invitée partie
        let connected = inner
            .guests
            .values()
            .filter(|guest| guest.left_at.is_none());
        let gone = inner
            .guests
            .values()
            .filter(|guest| guest.left_at.is_some());
        for guest in connected.chain(gone) {
            for session in &guest.sessions {
                if seen.insert(&session.id) {
                    sessions.push(session.clone());
                }
            }
        }
        self.snapshot.send_if_modified(|snapshot| {
            let changed = snapshot.sessions != sessions;
            if changed {
                snapshot.sessions = sessions;
            }
            changed
        });
    }

    /// Fait entrer la machine invitée `machine` (son identité réseau) par l'invitation `invite`.
    /// Retourne son numéro et le nom sous lequel ses sessions seront diffusées : celui de
    /// l'invitation, numéroté s'il est déjà pris (« cloud », « cloud 2 »). Une machine qui revient
    /// reprend son nom, et ses dernières sessions restent affichées jusqu'à son premier envoi.
    /// `None` : trop d'invitées connectées à la fois pour cette invitation.
    pub fn guest_join(&self, invite: &str, machine: &str) -> Option<(u64, String)> {
        let mut inner = self.inner.lock().unwrap();
        // La même machine, partie ou dont l'ancienne connexion n'a pas encore été constatée morte
        let previous = inner
            .guests
            .iter()
            .find(|(_, guest)| guest.invite == invite && guest.machine == machine)
            .map(|(id, _)| *id)
            .and_then(|id| inner.guests.remove(&id));
        let (host, sessions) = match previous {
            Some(guest) => (guest.host, guest.sessions),
            None => {
                let of_invite = |inner: &Inner| {
                    inner
                        .guests
                        .values()
                        .filter(|guest| guest.invite == invite)
                        .count()
                };
                // Plus de place : la partie la plus ancienne cède la sienne
                while of_invite(&inner) >= GUESTS_PER_INVITE {
                    let oldest = inner
                        .guests
                        .iter()
                        .filter(|(_, guest)| guest.invite == invite)
                        .filter_map(|(id, guest)| Some((guest.left_at?, *id)))
                        .min()?;
                    inner.guests.remove(&oldest.1);
                }
                let taken: Vec<&str> = inner
                    .guests
                    .values()
                    .filter(|guest| guest.invite == invite)
                    .map(|guest| guest.host.as_str())
                    .collect();
                let host = (1..)
                    .map(|n| match n {
                        1 => invite.to_string(),
                        n => format!("{invite} {n}"),
                    })
                    .find(|name| !taken.contains(&name.as_str()))?;
                (host, Vec::new())
            }
        };
        let id = inner.next_guest;
        inner.next_guest += 1;
        inner.guests.insert(
            id,
            Guest {
                invite: invite.to_string(),
                machine: machine.to_string(),
                left_at: None,
                host: host.clone(),
                sessions,
            },
        );
        self.broadcast(&inner);
        Some((id, host))
    }

    /// Remplace les sessions d'une invitée ; `false` si elle n'est plus admise (invitation
    /// révoquée, ou place reprise par une nouvelle connexion de la même machine).
    pub fn guest_update(&self, id: u64, sessions: Vec<Session>) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let Some(guest) = inner.guests.get_mut(&id) else {
            return false;
        };
        guest.sessions = sessions;
        self.broadcast(&inner);
        true
    }

    /// Une invitée est-elle encore admise ?
    pub fn guest_admitted(&self, id: u64) -> bool {
        self.inner.lock().unwrap().guests.contains_key(&id)
    }

    /// L'invitée est partie : ses dernières sessions connues restent affichées (D22). Ce qu'on ne
    /// peut plus affirmer est retiré : une session « au travail » n'a plus d'état connu, la
    /// machine ayant pu s'arrêter en plein tour.
    /// Retourne le nombre de sessions gardées ; `None` si elle n'était plus admise.
    pub fn guest_leave(&self, id: u64, now: u64) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();
        let guest = inner.guests.get_mut(&id)?;
        if guest.sessions.is_empty() {
            inner.guests.remove(&id);
            return Some(0);
        }
        let kept = guest.sessions.len();
        guest.left_at = Some(now);
        for session in &mut guest.sessions {
            if session.state == Some(State::Working) {
                session.state = None;
                session.tool = None;
                session.detail = None;
            }
            session.background_tasks = 0;
        }
        self.broadcast(&inner);
        Some(kept)
    }

    /// Oublie les invitées parties depuis plus de [`GUEST_KEPT`].
    pub fn guests_expire(&self, now: u64) {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.guests.len();
        inner.guests.retain(|_, guest| {
            guest
                .left_at
                .is_none_or(|left| now.saturating_sub(left) < GUEST_KEPT)
        });
        if inner.guests.len() != before {
            self.broadcast(&inner);
        }
    }

    /// Invitation révoquée : ses invitées sortent, celles déjà parties sont oubliées ; les
    /// connexions en cours le constatent et se ferment.
    pub fn guests_revoke(&self, invite: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.guests.retain(|_, guest| guest.invite != invite);
        self.broadcast(&inner);
    }

    /// Nombre d'invitées connectées par l'invitation `invite`.
    pub fn guests_count(&self, invite: &str) -> usize {
        let inner = self.inner.lock().unwrap();
        inner
            .guests
            .values()
            .filter(|guest| guest.invite == invite && guest.left_at.is_none())
            .count()
    }

    /// Change le nom de la machine et prévient les écrans.
    pub fn rename(&self, name: String) {
        let mut machine = self.machine.lock().unwrap();
        if *machine == name {
            return;
        }
        *machine = name.clone();
        drop(machine);
        self.snapshot
            .send_modify(|snapshot| snapshot.machine = name);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.subscribe()
    }
}

fn blank(id: &str, agent: &str) -> Session {
    Session {
        id: id.to_string(),
        agent: agent.to_string(),
        cwd: String::new(),
        title: None,
        state: None,
        tool: None,
        detail: None,
        last_message: None,
        url: None,
        model: None,
        background_tasks: 0,
        host: None,
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
            model: None,
        }
    }

    #[test]
    fn the_model_comes_from_hooks_and_from_local_cards() {
        let dir = std::env::temp_dir().join(format!("tallylight-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("a.jsonl");
        std::fs::write(
            &transcript,
            r#"{"type":"assistant","message":{"model":"claude-opus-5-5"}}"#,
        )
        .unwrap();
        let store = Store::new("m".into(), None);
        // Le chemin passe par l'encodeur JSON : sous Windows il contient des barres obliques inverses
        let stop = serde_json::json!({
            "session_id": "a",
            "hook_event_name": "Stop",
            "cwd": "/p",
            "transcript_path": transcript.to_string_lossy(),
        });
        store.apply_claude(&event(&stop.to_string()));
        assert_eq!(
            store.snapshot().sessions[0].model.as_deref(),
            Some("claude-opus-5-5")
        );

        // Une fiche sans modèle connu ne l'efface pas ; une fiche qui en a un le remplace
        let mut card = local(true, "busy", 10);
        let mut cards = HashMap::from([("a".to_string(), card.clone())]);
        store.reconcile(&cards, 10);
        assert_eq!(
            store.snapshot().sessions[0].model.as_deref(),
            Some("claude-opus-5-5")
        );
        card.model = Some("claude-fable-5-2".into());
        cards.insert("a".into(), card);
        store.reconcile(&cards, 11);
        assert_eq!(
            store.snapshot().sessions[0].model.as_deref(),
            Some("claude-fable-5-2")
        );
        let _ = std::fs::remove_dir_all(&dir);
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
    fn codex_sessions_live_next_to_claude_code_ones() {
        let store = Store::new("m".into(), None);
        store.apply_claude(&event(
            r#"{"session_id":"a","hook_event_name":"UserPromptSubmit","cwd":"/p/demo"}"#,
        ));
        store.apply_codex(&event(
            r#"{"session_id":"b","turn_id":"t","transcript_path":null,"cwd":"/p/api",
                "hook_event_name":"UserPromptSubmit","model":"gpt-example","permission_mode":"default","prompt":"x"}"#,
        ));
        store.apply_codex(&event(
            r#"{"session_id":"b","turn_id":"t","transcript_path":null,"cwd":"/p/api",
                "hook_event_name":"PermissionRequest","model":"gpt-example","permission_mode":"default",
                "tool_name":"apply_patch","tool_input":{"command":"*** Begin Patch\n*** Add File: src/new.rs\n+x\n*** End Patch"}}"#,
        ));
        let sessions = store.snapshot().sessions;
        assert_eq!(sessions[0].agent, "claude-code");
        let codex = &sessions[1];
        assert_eq!(
            (codex.agent.as_str(), codex.state, codex.model.as_deref()),
            ("codex", Some(State::InputRequired), Some("gpt-example"))
        );
        assert_eq!(
            (codex.tool.as_deref(), codex.detail.as_deref()),
            (Some("apply_patch"), Some("new.rs"))
        );

        // Aucune fiche locale pour Codex : le rapprochement avec celles de Claude Code garde la
        // session tant qu'elle a changé d'état depuis moins de 12 heures
        let t = now();
        store.reconcile(&HashMap::new(), t + 3600);
        assert_eq!(store.snapshot().sessions.len(), 2);

        store.apply_codex(&event(
            r#"{"session_id":"b","turn_id":"t","transcript_path":null,"cwd":"/p/api",
                "hook_event_name":"Interrupt","model":"gpt-example","permission_mode":"default"}"#,
        ));
        let codex = store.snapshot().sessions[1].clone();
        assert_eq!((codex.state, codex.tool), (Some(State::Canceled), None));
        store.apply_codex(&event(
            r#"{"session_id":"b","transcript_path":null,"cwd":"/p/api","hook_event_name":"SessionEnd","reason":"other"}"#,
        ));
        assert_eq!(store.snapshot().sessions.len(), 1);
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
    #[test]
    fn a_hosted_session_gets_its_link_from_the_environment() {
        assert_eq!(
            hosted_session_url(Some("true"), Some("cse_01AbC")).as_deref(),
            Some("https://claude.ai/code/session_01AbC")
        );
        // Hors session hébergée, ou identifiant inattendu : pas de lien inventé
        assert_eq!(hosted_session_url(None, Some("cse_01AbC")), None);
        assert_eq!(hosted_session_url(Some("true"), None), None);
        assert_eq!(hosted_session_url(Some("true"), Some("01AbC")), None);
        assert_eq!(hosted_session_url(Some("true"), Some("cse_")), None);
        assert_eq!(hosted_session_url(Some("true"), Some("cse_a/b?c")), None);
    }
}
