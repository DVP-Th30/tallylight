//! Ce que le cœur et un écran s'échangent sur leur connexion.
//!
//! L'écran ouvre un flux et envoie un [`Hello`] ; le cœur répond par un [`Welcome`], puis envoie un
//! [`Snapshot`](crate::Snapshot) à chaque changement. Chaque message tient sur une ligne JSON.

use serde::{Deserialize, Serialize};

/// Identifiant du protocole, échangé à l'ouverture de la connexion.
pub const ALPN: &[u8] = b"tallylight/state/1";

/// Première ligne envoyée par un écran : son nom, et un code s'il vient s'appairer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub name: String,
    #[serde(default)]
    pub code: Option<String>,
}

/// Identifiant du protocole d'envoi : une machine invitée pousse ses instantanés à une machine
/// d'accueil, qui les rediffuse à ses écrans (D21).
pub const FORWARD_ALPN: &[u8] = b"tallylight/forward/1";

/// Première ligne envoyée par une machine invitée : son nom et le secret de son invitation.
/// La machine d'accueil répond par un [`Welcome`] ; l'invitée envoie ensuite ses
/// [`Snapshot`](crate::Snapshot), un par ligne, à chaque changement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Join {
    pub name: String,
    pub invite: String,
}

/// Réponse du cœur ; les instantanés suivent si `ok` est vrai.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    pub ok: bool,
}
