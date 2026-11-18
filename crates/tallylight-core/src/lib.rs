//! Modèle partagé par le cœur et les écrans : états, sessions, instantané diffusé.
//! Voir `docs/DECISIONS.md` (D2) pour le choix du vocabulaire.

pub mod claude;
pub mod protocol;

use serde::{Deserialize, Serialize};

/// État du tour en cours d'une session, aligné sur `TaskState` d'A2A 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// L'agent travaille (A2A : `working`).
    Working,
    /// L'agent attend une réponse ou une autorisation (A2A : `input-required`).
    InputRequired,
    /// Le tour est terminé (A2A : `completed`).
    Completed,
    /// Le tour s'est terminé sur une erreur (A2A : `failed`).
    Failed,
    /// Le tour a été interrompu (A2A : `canceled`).
    Canceled,
}

impl State {
    /// Nom de l'état dans le vocabulaire A2A.
    pub fn as_a2a(self) -> &'static str {
        match self {
            State::Working => "working",
            State::InputRequired => "input-required",
            State::Completed => "completed",
            State::Failed => "failed",
            State::Canceled => "canceled",
        }
    }

    /// Vrai si l'agent a besoin de l'utilisateur pour continuer.
    pub fn needs_user(self) -> bool {
        matches!(self, State::InputRequired)
    }
}

/// Une session d'agent sur une machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    /// Agent qui tient la session (« claude-code »).
    pub agent: String,
    /// Dossier de travail de la session.
    pub cwd: String,
    /// Nom donné à la session par l'utilisateur ou l'agent, s'il y en a un.
    #[serde(default)]
    pub title: Option<String>,
    /// État du dernier tour ; `None` tant qu'aucun tour n'a commencé.
    pub state: Option<State>,
    /// Outil en cours d'exécution, quand l'agent travaille.
    pub tool: Option<String>,
    /// Dernier changement, en secondes depuis 1970.
    pub updated_at: u64,
}

impl Session {
    /// Nom à afficher : le titre de la session, sinon son dossier.
    pub fn label(&self) -> &str {
        self.title.as_deref().unwrap_or_else(|| self.project())
    }

    /// Nom court affiché : dernier élément du dossier de travail.
    pub fn project(&self) -> &str {
        self.cwd
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or("/")
    }
}

/// Ce qu'un écran reçoit : toutes les sessions d'une machine à un instant donné.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub machine: String,
    pub sessions: Vec<Session>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a2a_names_match_the_wire_format() {
        for state in [
            State::Working,
            State::InputRequired,
            State::Completed,
            State::Failed,
            State::Canceled,
        ] {
            assert_eq!(
                serde_json::to_string(&state).unwrap(),
                format!("\"{}\"", state.as_a2a())
            );
        }
        assert!(State::InputRequired.needs_user());
        assert!(!State::Working.needs_user());
    }

    #[test]
    fn project_is_the_last_folder() {
        let mut session = Session {
            id: "s".into(),
            agent: "claude-code".into(),
            cwd: "/home/th30/projets/tallylight/".into(),
            title: None,
            state: None,
            tool: None,
            updated_at: 0,
        };
        assert_eq!(session.project(), "tallylight");
        assert_eq!(session.label(), "tallylight");
        session.title = Some("Refonte du volet".into());
        assert_eq!(session.label(), "Refonte du volet");
        session.cwd = "/".into();
        assert_eq!(session.project(), "/");
    }
}
