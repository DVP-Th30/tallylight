//! Appareils autorisés à lire l'état, et codes d'appairage en attente.
//! Voir `docs/DECISIONS.md` (D8).

use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Durée de validité d'un code d'appairage.
const CODE_LIFETIME: Duration = Duration::from_secs(300);

/// Un écran autorisé, reconnu par son identité réseau (clé publique vérifiée à la connexion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// Date de l'appairage, en secondes depuis 1970.
    pub paired_at: u64,
}

pub struct Devices {
    path: PathBuf,
    devices: Vec<Device>,
    /// Codes à usage unique, avec leur heure limite.
    codes: HashMap<String, Instant>,
}

/// Réponse à un écran qui se présente.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    /// Déjà autorisé.
    Known,
    /// Autorisé à l'instant grâce à un code valide.
    Paired,
    Refused,
}

impl Devices {
    pub fn load(path: PathBuf) -> Self {
        let devices = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            devices,
            codes: HashMap::new(),
        }
    }

    pub fn list(&self) -> &[Device] {
        &self.devices
    }

    /// Crée un code d'appairage à usage unique.
    pub fn new_code(&mut self) -> Result<String> {
        let now = Instant::now();
        self.codes.retain(|_, deadline| *deadline > now);
        let mut bytes = [0u8; 10];
        getrandom::fill(&mut bytes)
            .map_err(|error| anyhow::anyhow!("aléa indisponible : {error}"))?;
        let code: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        self.codes.insert(code.clone(), now + CODE_LIFETIME);
        Ok(code)
    }

    /// Décide si l'écran `id` peut lire l'état. Un code valide l'ajoute aux appareils autorisés et est consommé.
    pub fn admit(
        &mut self,
        id: &str,
        name: &str,
        code: Option<&str>,
        now: u64,
    ) -> Result<Admission> {
        if self.devices.iter().any(|device| device.id == id) {
            return Ok(Admission::Known);
        }
        let valid = code
            .and_then(|code| self.codes.remove(code))
            .is_some_and(|deadline| deadline > Instant::now());
        if !valid {
            return Ok(Admission::Refused);
        }
        self.devices.push(Device {
            id: id.to_string(),
            name: name.to_string(),
            paired_at: now,
        });
        self.save()?;
        Ok(Admission::Paired)
    }

    /// Retire les appareils dont l'identité commence par `prefix` ; retourne ceux qui ont été retirés.
    pub fn remove(&mut self, prefix: &str) -> Result<Vec<Device>> {
        let (removed, kept): (Vec<Device>, Vec<Device>) = std::mem::take(&mut self.devices)
            .into_iter()
            .partition(|device| !prefix.is_empty() && device.id.starts_with(prefix));
        self.devices = kept;
        if !removed.is_empty() {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> Result<()> {
        crate::write_private(&self.path, &serde_json::to_vec_pretty(&self.devices)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices(name: &str) -> Devices {
        let path = std::env::temp_dir().join(format!(
            "tallylight-test-{name}-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Devices::load(path)
    }

    #[test]
    fn a_code_pairs_one_device_once() {
        let mut list = devices("once");
        let code = list.new_code().unwrap();
        assert_eq!(
            list.admit("aaa", "phone", None, 1).unwrap(),
            Admission::Refused
        );
        assert_eq!(
            list.admit("aaa", "phone", Some("wrong"), 1).unwrap(),
            Admission::Refused
        );
        assert_eq!(
            list.admit("aaa", "phone", Some(&code), 1).unwrap(),
            Admission::Paired
        );
        // Le code ne sert qu'une fois ; l'appareil appairé n'en a plus besoin
        assert_eq!(
            list.admit("bbb", "other", Some(&code), 2).unwrap(),
            Admission::Refused
        );
        assert_eq!(
            list.admit("aaa", "phone", None, 3).unwrap(),
            Admission::Known
        );

        // La liste survit à un redémarrage
        let reloaded = Devices::load(list.path.clone());
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].name, "phone");
        let _ = std::fs::remove_file(&list.path);
    }

    #[test]
    fn removing_a_device_closes_the_door() {
        let mut list = devices("remove");
        let code = list.new_code().unwrap();
        list.admit("abcdef", "phone", Some(&code), 1).unwrap();
        assert!(list.remove("").unwrap().is_empty());
        assert_eq!(list.remove("abc").unwrap().len(), 1);
        assert_eq!(
            list.admit("abcdef", "phone", None, 2).unwrap(),
            Admission::Refused
        );
        let _ = std::fs::remove_file(&list.path);
    }
}
