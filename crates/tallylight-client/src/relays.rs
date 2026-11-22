//! Choix des relais iroh : ceux de l'éditeur d'iroh (n0) par défaut, ou les siens.
//!
//! Un relais sert à établir la connexion entre un écran et une machine, et la porte tant que le
//! lien direct est impossible ; il ne voit que des paquets chiffrés de bout en bout. Les relais
//! publics de n0 sont réservés au développement (décision D4) : pour un usage réel, chacun indique
//! le sien. La machine annonce son relais dans son ticket d'appairage, donc un écran joint une
//! machine par le relais de celle-ci, quel que soit le sien.

use std::{fmt, str::FromStr};

use iroh::{RelayMode, RelayUrl, endpoint::Builder};

/// Relais utilisés par un point réseau (machine ou écran).
///
/// Forme texte, celle de `tallylight --relay` et de `TALLYLIGHT_RELAY` :
/// - `n0` (ou `default`, ou rien) : relais publics de n0, le comportement d'origine ;
/// - une ou plusieurs adresses séparées par des virgules : `https://relais.exemple.fr`,
///   `http://127.0.0.1:3340` (relais de développement, sans TLS).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relays(Choice);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Choice {
    /// Préréglage d'iroh, inchangé.
    #[default]
    N0,
    /// Ses propres relais, au moins un.
    Own(Vec<RelayUrl>),
}

/// Relais mal écrit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid relay: {0}")]
pub struct BadRelay(String);

impl Relays {
    /// Relais publics de n0 : pratique pour essayer, réservés au développement.
    pub fn n0() -> Self {
        Self(Choice::N0)
    }

    /// Vrai si ce sont les relais publics de n0.
    pub fn is_n0(&self) -> bool {
        self.0 == Choice::N0
    }

    /// Applique ce choix à la construction d'un point réseau iroh (partie d'un préréglage
    /// comme `presets::N0`). Les relais publics laissent le préréglage intact.
    pub fn apply(&self, builder: Builder) -> Builder {
        match &self.0 {
            Choice::N0 => builder,
            Choice::Own(urls) => builder.relay_mode(RelayMode::custom(urls.iter().cloned())),
        }
    }
}

impl FromStr for Relays {
    type Err = BadRelay;

    fn from_str(text: &str) -> Result<Self, BadRelay> {
        let text = text.trim();
        if matches!(text, "" | "n0" | "default") {
            return Ok(Self::n0());
        }
        let mut urls: Vec<RelayUrl> = Vec::new();
        for item in text.split(',').map(str::trim) {
            let url = relay_url(item)?;
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        Ok(Self(Choice::Own(urls)))
    }
}

/// Une adresse de relais : schéma http(s) et hôte, sans rien d'autre. Le chemin est fixé par iroh,
/// et l'adresse finit dans le ticket que l'on montre aux écrans : pas d'identifiants, pas de jeton.
fn relay_url(item: &str) -> Result<RelayUrl, BadRelay> {
    let bad = |reason: &str| BadRelay(format!("\"{item}\": {reason}"));
    if item.is_empty() {
        return Err(bad("empty address"));
    }
    if matches!(item, "n0" | "default") {
        return Err(bad("`n0` cannot be combined with other relays"));
    }
    let url = RelayUrl::from_str(item).map_err(|_| bad("unreadable address"))?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(bad(
            "an https:// address is required (or http:// for a local test)",
        ));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(bad("missing host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad("no credentials in the address"));
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
        return Err(bad("only the scheme, host and port"));
    }
    Ok(url)
}

impl fmt::Display for Relays {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Choice::N0 => f.write_str("n0"),
            Choice::Own(urls) => {
                let urls: Vec<String> = urls.iter().map(ToString::to_string).collect();
                f.write_str(&urls.join(","))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_relays_are_the_default() {
        for text in ["", "  ", "n0", "default"] {
            assert_eq!(text.parse::<Relays>().unwrap(), Relays::default());
        }
        assert!(Relays::default().is_n0());
        assert_eq!(Relays::default().to_string(), "n0");
    }

    #[test]
    fn own_relays_are_read_cleaned_and_written_back() {
        let relays: Relays =
            " https://relais.exemple.fr , http://127.0.0.1:3340,https://relais.exemple.fr/"
                .parse()
                .unwrap();
        assert!(!relays.is_n0());
        // Doublon retiré, forme canonique (barre finale) : c'est elle que `install` recopie
        assert_eq!(
            relays.to_string(),
            "https://relais.exemple.fr/,http://127.0.0.1:3340/"
        );
        assert_eq!(relays.to_string().parse::<Relays>().unwrap(), relays);
        // Un nom pleinement qualifié (point final, comme ceux de n0) est accepté
        assert!("https://relais.exemple.fr.".parse::<Relays>().is_ok());
        assert!("https://[::1]:3340".parse::<Relays>().is_ok());
    }

    #[test]
    fn malformed_relays_are_refused_with_the_reason() {
        let refused = |text: &str| text.parse::<Relays>().unwrap_err().to_string();
        assert!(refused("relais.exemple.fr").contains("unreadable"));
        assert!(refused("ftp://relais.exemple.fr").contains("https://"));
        assert!(refused("https://relais.exemple.fr,").contains("empty"));
        assert!(refused("n0,https://relais.exemple.fr").contains("cannot be combined"));
        assert!(refused("https://moi:secret@relais.exemple.fr").contains("credentials"));
        assert!(refused("https://relais.exemple.fr/?token=abc").contains("scheme, host"));
        assert!(refused("https://relais.exemple.fr/relay").contains("scheme, host"));
        assert!(refused("https://relais.exemple.fr/#x").contains("scheme, host"));
    }
}
