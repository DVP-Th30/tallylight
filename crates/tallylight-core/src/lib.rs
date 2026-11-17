//! Vocabulaire d'état d'une session d'agent, aligné sur `TaskState` d'A2A 1.0.
//! Voir `docs/DECISIONS.md` (D2) pour le choix de ce vocabulaire.

/// État d'une session d'agent, tel qu'un écran l'affiche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a2a_names() {
        assert_eq!(State::InputRequired.as_a2a(), "input-required");
        assert!(State::InputRequired.needs_user());
        assert!(!State::Working.needs_user());
    }
}
