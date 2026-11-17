//! tallylight : suit les sessions des agents de code d'une machine et diffuse leur état aux écrans appairés.
//!
//!     tallylight daemon [--port N]   # reçoit les hooks, tient l'état, sert les écrans
//!     tallylight watch [ticket]      # affiche l'état ; sans ticket, celui du démon local
//!     tallylight hooks [--port N]    # affiche les hooks à ajouter aux réglages de Claude Code

mod daemon;
mod hooks;
mod store;
mod watch;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// Identifiant du protocole entre le cœur et les écrans, échangé à l'ouverture de la connexion.
pub const ALPN: &[u8] = b"tallylight/state/0";

/// Port local où le démon reçoit les hooks.
pub const DEFAULT_PORT: u16 = 47700;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["daemon", rest @ ..] => daemon::run(port(rest)?).await,
        ["watch"] => watch::run(None).await,
        ["watch", ticket] => watch::run(Some(ticket)).await,
        ["hooks", rest @ ..] => {
            println!("{}", hooks::claude_settings(port(rest)?));
            Ok(())
        }
        _ => bail!("usage : tallylight daemon [--port N] | watch [ticket] | hooks [--port N]"),
    }
}

fn port(args: &[&str]) -> Result<u16> {
    match args {
        [] => Ok(DEFAULT_PORT),
        ["--port", value] => value.parse().context("port invalide"),
        _ => bail!("option inconnue : {}", args.join(" ")),
    }
}

/// Dossier des données de la machine : identité réseau et ticket d'appairage.
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
