//! tallylight : suit les sessions des agents de code d'une machine et diffuse leur état aux écrans appairés.
//!
//!     tallylight daemon [--port N]   # reçoit les hooks, tient l'état, sert les écrans appairés
//!     tallylight watch [appairage]   # affiche l'état ; sans argument, celui de cette machine
//!     tallylight pair                # affiche un QR et une chaîne d'appairage pour un nouvel écran
//!     tallylight devices             # liste les écrans appairés
//!     tallylight unpair <identité>   # retire un écran (début de son identité)
//!     tallylight install             # branche Claude Code sur le démon et le lance en permanence
//!     tallylight uninstall           # retire les hooks et le service
//!     tallylight hooks [--port N]    # affiche les hooks, pour les ajouter à la main

mod claude_local;
mod daemon;
mod devices;
mod hooks;
mod install;
mod local;
mod store;
mod watch;

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use iroh::SecretKey;

/// Port local où le démon reçoit les hooks et les commandes.
pub const DEFAULT_PORT: u16 = 47700;

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
        ["install"] => install::install(true),
        ["install", "--no-service"] => install::install(false),
        ["uninstall"] => install::uninstall(),
        ["hooks", rest @ ..] => {
            println!("{}", hooks::claude_settings(port(rest)?));
            Ok(())
        }
        _ => bail!(
            "usage : tallylight daemon [--port N] | watch [appairage] | pair | devices | unpair <identité> | install [--no-service] | uninstall | hooks [--port N]"
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

/// Dossier de configuration de Claude Code.
pub fn claude_dir() -> Result<PathBuf> {
    Ok(match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME").context("HOME absent")?).join(".claude"),
    })
}

/// Nom de la machine, tel qu'un écran l'affiche.
pub fn machine_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| {
            // macOS n'a pas ce fichier
            let output = std::process::Command::new("hostname").output().ok()?;
            String::from_utf8(output.stdout).ok()
        })
        .map(|name| name.trim().trim_end_matches(".local").to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "machine".to_string())
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
