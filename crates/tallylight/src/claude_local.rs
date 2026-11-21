//! Ce que Claude Code écrit sur la machine à propos de ses sessions en cours.
//!
//! Chaque processus tient une fiche `~/.claude/sessions/<pid>.json` (identifiant de session, dossier,
//! nom, statut `busy` / `waiting` / `idle`). Ce format est interne à Claude Code et peut changer sans
//! préavis : on s'en sert en complément des hooks, jamais comme source unique, et toute fiche
//! illisible est ignorée. Voir `docs/DECISIONS.md` (D9).

use std::{collections::HashMap, path::PathBuf};

use serde::Deserialize;

use crate::transcript::ModelCache;

/// Une session vue par ses fiches locales (plusieurs processus peuvent porter la même session).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalSession {
    /// Vrai si au moins un processus de cette session existe encore.
    pub alive: bool,
    pub cwd: String,
    /// Nom choisi pour la session ; absent si Claude Code l'a seulement dérivé du dossier.
    pub title: Option<String>,
    /// Statut du processus vivant le plus récent.
    pub status: Option<String>,
    /// Date de ce statut, en secondes depuis 1970.
    pub status_at: u64,
    /// Adresse de la session dans l'application Claude, quand elle est suivie à distance.
    pub url: Option<String>,
    /// Dernier modèle qui a répondu, lu dans la transcription (voir `transcript`).
    pub model: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Card {
    pid: u32,
    session_id: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    name_source: Option<String>,
    #[serde(default)]
    status: Option<String>,
    /// En millisecondes.
    #[serde(default)]
    status_updated_at: u64,
    /// Identifiant de la session côté claude.ai (Remote Control), de la forme `session_…`.
    #[serde(default)]
    bridge_session_id: Option<String>,
}

/// Lit les fiches de la machine, regroupées par identifiant de session, avec le modèle de chaque
/// session vivante.
pub fn read_with_models(models: &mut ModelCache) -> HashMap<String, LocalSession> {
    let mut sessions = read();
    for (id, session) in sessions.iter_mut().filter(|(_, session)| session.alive) {
        session.model = models.model(&session.cwd, id);
    }
    models.retain(|path| {
        sessions.iter().any(|(id, session)| {
            session.alive && path.file_stem().is_some_and(|stem| stem == id.as_str())
        })
    });
    sessions
}

/// Lit les fiches de la machine, regroupées par identifiant de session.
pub fn read() -> HashMap<String, LocalSession> {
    let mut sessions: HashMap<String, LocalSession> = HashMap::new();
    let Some(dir) = sessions_dir() else {
        return sessions;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return sessions;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Some(card) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Card>(&bytes).ok())
        else {
            continue;
        };
        merge(&mut sessions, card, alive);
    }
    sessions
}

fn merge(sessions: &mut HashMap<String, LocalSession>, card: Card, alive: fn(u32) -> bool) {
    let alive = alive(card.pid);
    let session = sessions.entry(card.session_id).or_default();
    let status_at = card.status_updated_at / 1000;
    // Un processus vivant l'emporte sur une fiche laissée par un processus mort
    if alive && (!session.alive || status_at >= session.status_at) {
        session.cwd = card.cwd;
        session.status = card.status;
        session.status_at = status_at;
        session.title = card
            .name
            .filter(|_| card.name_source.as_deref() != Some("derived"));
        session.url = card
            .bridge_session_id
            .filter(|id| id.starts_with("session_"))
            .map(|id| format!("https://claude.ai/code/{id}"));
    }
    session.alive |= alive;
}

fn sessions_dir() -> Option<PathBuf> {
    Some(crate::claude_dir().ok()?.join("sessions"))
}

/// Vrai si le processus existe (même s'il appartient à un autre utilisateur).
#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // Signal 0 : aucune action, seulement le contrôle d'existence
    let sent = unsafe { libc::kill(pid, 0) };
    sent == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(json: &str) -> Card {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_live_process_wins_over_a_stale_card() {
        let mut sessions = HashMap::new();
        merge(
            &mut sessions,
            card(
                r#"{"pid":1,"sessionId":"s","cwd":"/old","status":"busy","statusUpdatedAt":9000}"#,
            ),
            |_| false,
        );
        assert!(!sessions["s"].alive);
        merge(
            &mut sessions,
            card(
                r#"{"pid":2,"sessionId":"s","cwd":"/new","name":"Refonte","nameSource":"user","status":"idle","statusUpdatedAt":5000,"bridgeSessionId":"session_abc"}"#,
            ),
            |_| true,
        );
        let session = &sessions["s"];
        assert!(session.alive);
        assert_eq!(session.cwd, "/new");
        assert_eq!(session.title.as_deref(), Some("Refonte"));
        assert_eq!(session.status.as_deref(), Some("idle"));
        assert_eq!(session.status_at, 5);
        assert_eq!(
            session.url.as_deref(),
            Some("https://claude.ai/code/session_abc")
        );
    }

    #[test]
    fn a_derived_name_is_not_a_title() {
        let mut sessions = HashMap::new();
        merge(
            &mut sessions,
            card(r#"{"pid":2,"sessionId":"s","name":"projets-2e","nameSource":"derived"}"#),
            |_| true,
        );
        assert_eq!(sessions["s"].title, None);
    }
}
