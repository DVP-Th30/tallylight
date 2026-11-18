//! `tallylight install` : branche Claude Code sur le démon et fait tourner celui-ci en permanence.
//!
//! Deux choses, toutes deux réversibles par `tallylight uninstall` :
//! - les hooks, ajoutés aux réglages de Claude Code à côté de ceux qui existent (sauvegarde à côté) ;
//! - le service : unité utilisateur systemd sous Linux, agent de lancement sous macOS.

use std::{
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{claude_dir, hooks, local_port};

#[cfg(not(target_os = "macos"))]
const SERVICE: &str = "tallylight";
#[cfg(target_os = "macos")]
const LAUNCH_LABEL: &str = "dev.tallylight.daemon";

pub fn install(with_service: bool) -> Result<()> {
    let settings = settings_path()?;
    let changed = edit_settings(&settings, |value| hooks::add_claude(value, local_port()))?;
    println!(
        "hooks Claude Code : {} ({})",
        if changed {
            "ajoutés"
        } else {
            "déjà en place"
        },
        settings.display()
    );
    if with_service {
        install_service()?;
    }
    println!("\nPour suivre cette machine depuis un autre écran : tallylight pair");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let settings = settings_path()?;
    let changed = edit_settings(&settings, |value| hooks::remove_claude(value, local_port()))?;
    println!(
        "hooks Claude Code : {}",
        if changed {
            "retirés"
        } else {
            "aucun à retirer"
        }
    );
    uninstall_service()?;
    println!("Les données (identité, écrans appairés) restent dans le dossier de tallylight.");
    Ok(())
}

fn settings_path() -> Result<PathBuf> {
    Ok(claude_dir()?.join("settings.json"))
}

/// Applique `edit` aux réglages et n'écrit le fichier que s'il a changé, après une sauvegarde.
fn edit_settings(path: &PathBuf, edit: impl FnOnce(&mut Value) -> bool) -> Result<bool> {
    let original = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::from("{}"),
        Err(error) => return Err(error).context("lecture des réglages de Claude Code"),
    };
    let mut value: Value = serde_json::from_str(&original).with_context(|| {
        format!(
            "{} n'est pas un JSON valide ; rien n'a été modifié",
            path.display()
        )
    })?;
    if !edit(&mut value) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let backup = path.with_extension("json.avant-tallylight");
    if !backup.exists() && path.exists() {
        std::fs::copy(path, &backup)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&value)? + "\n")?;
    Ok(true)
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("lancement de {program}"))?;
    if !status.success() {
        bail!("{program} {} a échoué", args.join(" "));
    }
    Ok(())
}

/// Attend que le démon réponde sur son port local. Il ne l'ouvre qu'une fois joint au réseau
/// (quelques secondes) : sans cette attente, un `tallylight pair` lancé aussitôt après
/// l'installation trouve porte close.
fn wait_for_daemon(port: u16, patience: Duration) -> bool {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let deadline = Instant::now() + patience;
    loop {
        if TcpStream::connect_timeout(&address, Duration::from_millis(500)).is_ok() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Dit si le service lancé répond vraiment, plutôt que d'annoncer « démarré » sur la foi du lanceur.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn report_service(journal: &str) {
    // Le service écoute sur le port par défaut, quel que soit TALLYLIGHT_PORT ici
    if wait_for_daemon(crate::DEFAULT_PORT, Duration::from_secs(20)) {
        println!("service : démarré ({journal})");
    } else {
        println!(
            "service : lancé, mais le démon ne répond pas encore sur le port {} ; \
             il attend peut-être le réseau, ou il a échoué ({journal})",
            crate::DEFAULT_PORT
        );
    }
}

#[cfg(target_os = "linux")]
fn install_service() -> Result<()> {
    let exe = std::env::current_exe()?;
    let dir = PathBuf::from(std::env::var_os("HOME").context("HOME absent")?)
        .join(".config/systemd/user");
    std::fs::create_dir_all(&dir)?;
    let unit = format!(
        "[Unit]\nDescription=tallylight : suivi des sessions d'agents de code\nAfter=network-online.target\n\n\
         [Service]\nExecStart={} daemon\nRestart=on-failure\nRestartSec=5\nMemoryMax=200M\n\n\
         [Install]\nWantedBy=default.target\n",
        exe.display()
    );
    std::fs::write(dir.join(format!("{SERVICE}.service")), unit)?;
    run("systemctl", &["--user", "daemon-reload"])?;
    run("systemctl", &["--user", "enable", "--now", SERVICE])?;
    run("systemctl", &["--user", "restart", SERVICE])?;
    report_service(&format!("systemctl --user status {SERVICE}"));
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_service() -> Result<()> {
    let unit = PathBuf::from(std::env::var_os("HOME").context("HOME absent")?)
        .join(".config/systemd/user")
        .join(format!("{SERVICE}.service"));
    if unit.exists() {
        let _ = run("systemctl", &["--user", "disable", "--now", SERVICE]);
        std::fs::remove_file(unit)?;
        let _ = run("systemctl", &["--user", "daemon-reload"]);
        println!("service : arrêté et retiré");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_agent() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME absent")?)
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCH_LABEL}.plist")),
    )
}

#[cfg(target_os = "macos")]
fn install_service() -> Result<()> {
    let exe = std::env::current_exe()?;
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME absent")?);
    let log = home.join("Library/Logs/tallylight.log");
    let plist = launch_agent()?;
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LAUNCH_LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{}</string><string>daemon</string></array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
            exe.display(),
            log = log.display()
        ),
    )?;
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    // Recharge proprement si l'agent tournait déjà
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{LAUNCH_LABEL}")])
        .output();
    run(
        "launchctl",
        &["bootstrap", &domain, &plist.to_string_lossy()],
    )?;
    report_service(&format!("journal : {}", log.display()));
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_service() -> Result<()> {
    let plist = launch_agent()?;
    if plist.exists() {
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("{domain}/{LAUNCH_LABEL}")])
            .output();
        std::fs::remove_file(plist)?;
        println!("service : arrêté et retiré");
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn install_service() -> Result<()> {
    println!(
        "service : non géré sur ce système ; lancer `{SERVICE} daemon` au démarrage de la session"
    );
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn uninstall_service() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_backed_up_once_and_only_written_on_change() {
        let dir = std::env::temp_dir().join(format!("tallylight-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let original = "{\n  \"theme\": \"dark\",\n  \"language\": \"francais\"\n}\n";
        std::fs::write(&path, original).unwrap();

        assert!(edit_settings(&path, |value| hooks::add_claude(value, 4242)).unwrap());
        let backup = path.with_extension("json.avant-tallylight");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
        let written = std::fs::read_to_string(&path).unwrap();
        // L'ordre des réglages existants est conservé, les hooks arrivent à la fin
        assert!(written.find("theme").unwrap() < written.find("language").unwrap());
        assert!(written.find("language").unwrap() < written.find("hooks").unwrap());

        // Deuxième passage : rien ne change, le fichier n'est pas réécrit
        assert!(!edit_settings(&path, |value| hooks::add_claude(value, 4242)).unwrap());

        // Retrait : on retrouve les réglages d'origine
        assert!(edit_settings(&path, |value| hooks::remove_claude(value, 4242)).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waiting_for_the_daemon_sees_a_port_that_opens_late() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // Personne n'écoute : l'attente s'arrête à l'échéance
        assert!(!wait_for_daemon(port, Duration::from_millis(300)));
        let late = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap();
            let _ = listener.accept();
        });
        assert!(wait_for_daemon(port, Duration::from_secs(10)));
        late.join().unwrap();
    }

    #[test]
    fn a_broken_settings_file_is_left_alone() {
        let dir =
            std::env::temp_dir().join(format!("tallylight-install-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ pas du json").unwrap();
        assert!(edit_settings(&path, |value| hooks::add_claude(value, 4242)).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ pas du json");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
