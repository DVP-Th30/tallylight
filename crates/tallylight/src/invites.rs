//! Invitations : le droit, pour une machine éphémère (session hébergée en ligne, conteneur),
//! d'envoyer l'état de ses sessions à cette machine, qui le rediffuse à ses écrans.
//! Voir `docs/DECISIONS.md` (D21).

use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Préfixe du secret d'une invitation : le distingue d'un code d'appairage dans une chaîne
/// « ticket#secret ».
pub const SECRET_PREFIX: &str = "inv";

/// Longueur maximale du nom d'une invitation.
const NAME_MAX: usize = 32;

/// Une invitation. Sert autant de fois et aussi longtemps qu'elle existe ; `tallylight revoke`
/// la retire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    /// Nom donné par l'utilisateur : c'est sous ce nom que les machines invitées apparaissent.
    pub name: String,
    /// Secret présenté par la machine invitée. Jamais réaffiché après la création.
    pub secret: String,
    /// Date de création, en secondes depuis 1970.
    pub created_at: u64,
}

pub struct Invites {
    path: PathBuf,
    invites: Vec<Invite>,
}

impl Invites {
    pub fn load(path: PathBuf) -> Self {
        let invites = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, invites }
    }

    pub fn list(&self) -> &[Invite] {
        &self.invites
    }

    /// Crée l'invitation `name` et retourne son secret.
    pub fn create(&mut self, name: &str, now: u64) -> Result<String> {
        if !valid_name(name) {
            bail!(
                "an invitation name is 1 to {NAME_MAX} letters, digits, `-` or `_` (for example: cloud)"
            );
        }
        if self.invites.iter().any(|invite| invite.name == name) {
            bail!(
                "an invitation named \"{name}\" already exists: revoke it first, or choose another name"
            );
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|error| anyhow::anyhow!("no source of randomness: {error}"))?;
        let secret: String = SECRET_PREFIX
            .chars()
            .chain(bytes.iter().flat_map(|byte| {
                [byte >> 4, byte & 15].map(|n| char::from_digit(n.into(), 16).unwrap())
            }))
            .collect();
        self.invites.push(Invite {
            name: name.to_string(),
            secret: secret.clone(),
            created_at: now,
        });
        self.save()?;
        Ok(secret)
    }

    /// Nom de l'invitation qui porte ce secret, si elle existe encore.
    pub fn check(&self, secret: &str) -> Option<&str> {
        self.invites
            .iter()
            .find(|invite| same(invite.secret.as_bytes(), secret.as_bytes()))
            .map(|invite| invite.name.as_str())
    }

    /// Retire l'invitation `name` ; `false` si elle n'existait pas.
    pub fn remove(&mut self, name: &str) -> Result<bool> {
        let before = self.invites.len();
        self.invites.retain(|invite| invite.name != name);
        if self.invites.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    fn save(&self) -> Result<()> {
        crate::write_private(&self.path, &serde_json::to_vec_pretty(&self.invites)?)
    }
}

/// Lettres, chiffres, tiret et tiret bas : le nom passe tel quel dans une adresse locale et
/// dans une ligne de commande.
fn valid_name(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Comparaison dont la durée ne dépend pas de l'endroit où les deux secrets diffèrent.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invites(name: &str) -> Invites {
        let path = std::env::temp_dir().join(format!(
            "tallylight-test-invites-{name}-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Invites::load(path)
    }

    #[test]
    fn an_invitation_works_until_it_is_revoked() {
        let mut list = invites("life");
        let secret = list.create("cloud", 1).unwrap();
        assert!(secret.starts_with(SECRET_PREFIX));
        assert_eq!(secret.len(), SECRET_PREFIX.len() + 32);
        // Sert autant de fois qu'on veut, et survit à un redémarrage
        assert_eq!(list.check(&secret), Some("cloud"));
        assert_eq!(list.check(&secret), Some("cloud"));
        assert_eq!(
            Invites::load(list.path.clone()).check(&secret),
            Some("cloud")
        );
        assert_eq!(list.check("inv00"), None);
        assert_eq!(list.check(""), None);

        // Un nom ne sert qu'une fois ; deux invitations ont des secrets différents
        assert!(list.create("cloud", 2).is_err());
        let other = list.create("ci", 2).unwrap();
        assert_ne!(other, secret);

        assert!(list.remove("cloud").unwrap());
        assert!(!list.remove("cloud").unwrap());
        assert_eq!(list.check(&secret), None);
        assert_eq!(list.check(&other), Some("ci"));
        let _ = std::fs::remove_file(&list.path);
    }

    #[test]
    fn names_stay_plain() {
        let mut list = invites("names");
        for bad in ["", "a b", "a/b", "é", "a#b", &"x".repeat(33)] {
            assert!(list.create(bad, 1).is_err(), "{bad:?}");
        }
        assert!(list.create("ci_runner-2", 1).is_ok());
        let _ = std::fs::remove_file(&list.path);
    }
}
