//! tallylight : suit les sessions des agents de code d'une machine et diffuse leur état aux écrans appairés.
//!
//!     tallylight daemon [--port N]   # reçoit les hooks, tient l'état, sert les écrans appairés
//!     tallylight watch [appairage]   # affiche l'état ; sans argument, celui de cette machine
//!     tallylight pair                # affiche un QR et une chaîne d'appairage pour un nouvel écran
//!     tallylight devices             # liste les écrans appairés
//!     tallylight unpair <identité>   # retire un écran (début de son identité)
//!     tallylight hooks [--port N]    # affiche les hooks à ajouter aux réglages de Claude Code

mod daemon;
mod devices;
mod hooks;
mod local;
mod store;
mod watch;

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use iroh::SecretKey;
use serde::{Deserialize, Serialize};

/// Identifiant du protocole entre le cœur et les écrans, échangé à l'ouverture de la connexion.
pub const ALPN: &[u8] = b"tallylight/state/1";

/// Port local où le démon reçoit les hooks et les commandes.
pub const DEFAULT_PORT: u16 = 47700;

/// Première ligne envoyée par un écran : son nom, et un code s'il vient s'appairer.
#[derive(Debug, Serialize, Deserialize)]
pub struct Hello {
    pub name: String,
    #[serde(default)]
    pub code: Option<String>,
}

/// Réponse du cœur ; les instantanés suivent si `ok` est vrai.
#[derive(Debug, Serialize, Deserialize)]
pub struct Welcome {
    pub ok: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["daemon", rest @ ..] => daemon::run(port(rest)?).await,
        ["watch"] => watch::run(None).await,
        ["watch", pairing] => watch::run(Some(pairing)).await,
        ["pair"] => local::pair().await,
        ["devices"] => local::devices().await,
        ["unpair", prefix] => local::unpair(prefix).await,
        ["hooks", rest @ ..] => {
            println!("{}", hooks::claude_settings(port(rest)?));
            Ok(())
        }
        _ => bail!(
            "usage : tallylight daemon [--port N] | watch [appairage] | pair | devices | unpair <identité> | hooks [--port N]"
        ),
    }
}

/// Port du démon de cette machine ; `TALLYLIGHT_PORT` le remplace (essais, second démon).
pub fn local_port() -> u16 {
    std::env::var("TALLYLIGHT_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn port(args: &[&str]) -> Result<u16> {
    match args {
        [] => Ok(local_port()),
        ["--port", value] => value.parse().context("port invalide"),
        _ => bail!("option inconnue : {}", args.join(" ")),
    }
}

/// Dossier des données de la machine : identités réseau, ticket, appareils appairés.
pub fn data_dir() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => {
            PathBuf::from(std::env::var_os("HOME").context("HOME absent")?).join(".local/share")
        }
    };
    let dir = base.join("tallylight");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Identité réseau conservée dans le fichier `name`, créée au premier usage : un cœur garde le même
/// ticket d'un lancement à l'autre, un écran reste reconnu après son appairage.
pub fn identity(name: &str) -> Result<SecretKey> {
    let path = data_dir()?.join(name);
    if let Ok(bytes) = std::fs::read(&path) {
        let bytes: [u8; 32] = bytes.try_into().ok().context("clé d'identité illisible")?;
        return Ok(SecretKey::from_bytes(&bytes));
    }
    let key = SecretKey::generate();
    write_private(&path, &key.to_bytes())?;
    Ok(key)
}

/// Écrit un fichier lisible par son seul propriétaire.
pub fn write_private(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Secondes depuis 1970.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
