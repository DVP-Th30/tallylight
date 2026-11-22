//! `tallylight install` : branche les agents sur le démon et fait tourner celui-ci en permanence.
//!
//! Deux choses, toutes deux réversibles par `tallylight uninstall` :
//! - les hooks, ajoutés à côté de ceux qui existent (sauvegarde à côté) : dans les réglages de
//!   Claude Code, et dans le `hooks.json` de Codex si Codex est présent sur la machine ;
//! - le service : unité utilisateur systemd sous Linux, agent de lancement sous macOS.

use std::{
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tallylight_client::Relays;

use crate::{claude_dir, codex_dir, hooks, local_port};

#[cfg(not(target_os = "macos"))]
const SERVICE: &str = "tallylight";
#[cfg(target_os = "macos")]
const LAUNCH_LABEL: &str = "dev.tallylight.daemon";

/// `relay` : relais demandés par `--relay` ou `TALLYLIGHT_RELAY` ; sans demande, le service garde
/// ceux qu'il avait (une mise à jour par `install` ne doit pas les perdre), sinon ceux de n0.
pub fn install(with_service: bool, relay: Option<Relays>) -> Result<()> {
    let settings = settings_path()?;
    let changed = edit_settings(&settings, |value| hooks::add_claude(value, local_port()))?;
    println!(
        "Claude Code hooks: {} ({})",
        if changed { "added" } else { "already in place" },
        settings.display()
    );
    install_codex()?;
    if with_service {
        let previous = service_file()
            .ok()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| relay_in_service(&text));
        let relays = relay.or(previous).unwrap_or_default();
        if relays.is_n0() {
            println!("relay: n0, public and meant for development (--relay <url> to use your own)");
        } else {
            println!("relay: {relays}");
        }
        install_service(&daemon_args(&relays))?;
    } else if relay.is_some() {
        println!("relay: ignored without the service (--no-service)");
    }
    println!("\nTo follow this machine from another screen: tallylight pair");
    Ok(())
}

/// Arguments du démon lancé par le service. Les relais de n0 n'ajoutent rien : le service reste
/// alors identique à celui des versions précédentes.
fn daemon_args(relays: &Relays) -> Vec<String> {
    let mut args = vec!["daemon".to_string()];
    if !relays.is_n0() {
        // Forme canonique, vérifiée à la lecture : schéma, hôte et port seulement, donc aucun
        // caractère à protéger dans une ligne systemd ni dans un plist
        args.extend(["--relay".to_string(), relays.to_string()]);
    }
    args
}

/// Relais inscrits dans un fichier de service écrit par `install` (unité systemd ou plist).
fn relay_in_service(text: &str) -> Option<Relays> {
    let text = text.replace("<string>", " ").replace("</string>", " ");
    let mut words = text.split_whitespace();
    words.find(|word| *word == "--relay")?;
    words.next()?.parse().ok()
}

/// Échappe un texte pour un plist (XML).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Unité systemd utilisateur du démon.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn systemd_unit(exe: &str, args: &[String]) -> String {
    format!(
        "[Unit]\nDescription=tallylight: state of coding agent sessions\nAfter=network-online.target\n\n\
         [Service]\nExecStart={exe} {}\nRestart=on-failure\nRestartSec=5\nMemoryMax=200M\n\n\
         [Install]\nWantedBy=default.target\n",
        args.join(" ")
    )
}

/// Agent de lancement macOS du démon.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launch_agent_plist(label: &str, exe: &str, args: &[String], log: &str) -> String {
    let arguments: String = std::iter::once(exe)
        .chain(args.iter().map(String::as_str))
        .map(|arg| format!("<string>{}</string>", xml(arg)))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{label}</string>
    <key>ProgramArguments</key>
    <array>{arguments}</array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardOutPath</key><string>{log}</string>
    <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        log = xml(log)
    )
}

/// Ajoute les hooks de Codex, seulement si Codex est présent : on ne crée pas de dossier `.codex`
/// sur une machine qui ne s'en sert pas.
fn install_codex() -> Result<()> {
    let dir = codex_dir()?;
    if !dir.is_dir() && !on_path("codex") {
        println!("Codex hooks: skipped (Codex not found on this machine)");
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let path = dir.join("hooks.json");
    let changed = edit_settings(&path, |value| {
        hooks::add_codex(value, &exe.to_string_lossy(), local_port())
    })?;
    println!(
        "Codex hooks: {} ({})",
        if changed { "added" } else { "already in place" },
        path.display()
    );
    if changed {
        // Codex ignore un hook nouveau ou modifié tant que l'utilisateur ne l'a pas approuvé
        println!(
            "  Codex skips new hooks until you trust them: open Codex and run /hooks to review them"
        );
    }
    let inline = std::fs::read_to_string(dir.join("config.toml"))
        .is_ok_and(|config| declares_hooks(&config));
    if inline {
        println!(
            "  note: config.toml also declares hooks; Codex loads both files and says so at startup"
        );
    }
    Ok(())
}

/// Vrai si un `config.toml` de Codex déclare lui-même des hooks. La table `hooks.state`, où Codex
/// note les hooks approuvés, n'en est pas une déclaration.
fn declares_hooks(config: &str) -> bool {
    config.lines().map(str::trim_start).any(|line| {
        let table = line.trim_start_matches('[');
        line.starts_with('[')
            && (table.starts_with("hooks]") || table.starts_with("hooks."))
            && !table.starts_with("hooks.state")
    })
}

/// Vrai si un programme de ce nom est dans le `PATH`.
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// Retire les hooks de Codex. Un `hooks.json` qui ne contenait qu'eux est supprimé : rien ne reste
/// d'une installation faite sur une machine où ce fichier n'existait pas.
fn uninstall_codex() -> Result<()> {
    let path = codex_dir()?.join("hooks.json");
    if !path.exists() {
        return Ok(());
    }
    let changed = remove_codex_hooks(&path, local_port())?;
    println!(
        "Codex hooks: {}",
        if changed { "removed" } else { "none to remove" }
    );
    Ok(())
}

fn remove_codex_hooks(path: &PathBuf, port: u16) -> Result<bool> {
    // Sans sauvegarde : celle de l'installation, s'il y avait un fichier avant nous, suffit, et
    // une copie faite maintenant garderait nos propres hooks
    let changed = edit_file(path, false, |value| hooks::remove_codex(value, port))?;
    if changed && std::fs::read_to_string(path).is_ok_and(|text| text.trim() == "{}") {
        std::fs::remove_file(path)?;
    }
    Ok(changed)
}

pub fn uninstall() -> Result<()> {
    let settings = settings_path()?;
    let changed = edit_settings(&settings, |value| hooks::remove_claude(value, local_port()))?;
    println!(
        "Claude Code hooks: {}",
        if changed { "removed" } else { "none to remove" }
    );
    uninstall_codex()?;
    uninstall_service()?;
    println!("Data (identity, paired screens) stays in the tallylight data folder.");
    Ok(())
}

fn settings_path() -> Result<PathBuf> {
    Ok(claude_dir()?.join("settings.json"))
}

/// Applique `edit` aux réglages et n'écrit le fichier que s'il a changé, après une sauvegarde.
fn edit_settings(path: &PathBuf, edit: impl FnOnce(&mut Value) -> bool) -> Result<bool> {
    edit_file(path, true, edit)
}

/// Comme [`edit_settings`] ; `backup` : garder une copie du fichier d'avant, s'il n'y en a pas déjà.
fn edit_file(path: &PathBuf, backup: bool, edit: impl FnOnce(&mut Value) -> bool) -> Result<bool> {
    let original = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::from("{}"),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", path.display()));
        }
    };
    let mut value: Value = serde_json::from_str(&original)
        .with_context(|| format!("{} is not valid JSON; nothing was changed", path.display()))?;
    if !edit(&mut value) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let copy = path.with_extension("json.before-tallylight");
    if backup && !copy.exists() && path.exists() {
        std::fs::copy(path, &copy)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&value)? + "\n")?;
    Ok(true)
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("could not run {program}"))?;
    if !status.success() {
        bail!("{program} {} failed", args.join(" "));
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
        println!("service: started ({journal})");
    } else {
        println!(
            "service: launched, but the daemon is not answering on port {} yet; \
             it may be waiting for the network, or it failed ({journal})",
            crate::DEFAULT_PORT
        );
    }
}

#[cfg(target_os = "linux")]
fn service_file() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
            .join(".config/systemd/user")
            .join(format!("{SERVICE}.service")),
    )
}

#[cfg(target_os = "linux")]
fn install_service(args: &[String]) -> Result<()> {
    let exe = std::env::current_exe()?;
    let unit = service_file()?;
    if let Some(dir) = unit.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&unit, systemd_unit(&exe.display().to_string(), args))?;
    run("systemctl", &["--user", "daemon-reload"])?;
    run("systemctl", &["--user", "enable", "--now", SERVICE])?;
    run("systemctl", &["--user", "restart", SERVICE])?;
    report_service(&format!("systemctl --user status {SERVICE}"));
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_service() -> Result<()> {
    let unit = service_file()?;
    if unit.exists() {
        let _ = run("systemctl", &["--user", "disable", "--now", SERVICE]);
        std::fs::remove_file(unit)?;
        let _ = run("systemctl", &["--user", "daemon-reload"]);
        println!("service: stopped and removed");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn service_file() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCH_LABEL}.plist")),
    )
}

#[cfg(target_os = "macos")]
fn install_service(args: &[String]) -> Result<()> {
    let exe = std::env::current_exe()?;
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let log = home.join("Library/Logs/tallylight.log");
    let plist = service_file()?;
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &plist,
        launch_agent_plist(
            LAUNCH_LABEL,
            &exe.display().to_string(),
            args,
            &log.display().to_string(),
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
    report_service(&format!("log: {}", log.display()));
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_service() -> Result<()> {
    let plist = service_file()?;
    if plist.exists() {
        let domain = format!("gui/{}", unsafe { libc::getuid() });
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("{domain}/{LAUNCH_LABEL}")])
            .output();
        std::fs::remove_file(plist)?;
        println!("service: stopped and removed");
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn service_file() -> Result<PathBuf> {
    bail!("no service support on this system")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn install_service(args: &[String]) -> Result<()> {
    println!(
        "service: not supported on this system; run `{SERVICE} {}` when your session starts",
        args.join(" ")
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
        let backup = path.with_extension("json.before-tallylight");
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
    fn codex_hooks_are_backed_up_and_removed_without_a_trace() {
        let dir = std::env::temp_dir().join(format!("tallylight-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hooks.json");
        let original = "{\n  \"description\": \"mine\",\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"say done\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";
        std::fs::write(&path, original).unwrap();

        assert!(
            edit_settings(&path, |value| hooks::add_codex(
                value,
                "/bin/tallylight",
                4242
            ))
            .unwrap()
        );
        let backup = dir.join("hooks.json.before-tallylight");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("say done"));
        assert!(written.contains("/bin/tallylight hook codex --port 4242"));
        assert!(
            !edit_settings(&path, |value| hooks::add_codex(
                value,
                "/bin/tallylight",
                4242
            ))
            .unwrap()
        );

        assert!(remove_codex_hooks(&path, 4242).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

        // Machine où Codex n'avait pas de `hooks.json` : après retrait, ni fichier ni sauvegarde
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(&backup).unwrap();
        assert!(
            edit_settings(&path, |value| hooks::add_codex(
                value,
                "/bin/tallylight",
                4242
            ))
            .unwrap()
        );
        assert!(path.exists() && !backup.exists());
        assert!(remove_codex_hooks(&path, 4242).unwrap());
        assert!(!path.exists() && !backup.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inline_codex_hooks_are_noticed_but_not_the_trust_state() {
        assert!(declares_hooks(
            "model = \"x\"\n\n[[hooks.PreToolUse]]\nmatcher = \"Bash\"\n"
        ));
        assert!(declares_hooks("  [hooks]\nStop = []\n"));
        assert!(!declares_hooks(
            "[hooks.state.\"a:b\"]\ntrusted_hash = \"sha256:x\"\n"
        ));
        assert!(!declares_hooks("[features]\nhooks = true\n[hooksmith]\n"));
        assert!(!declares_hooks(""));
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
    fn the_service_is_unchanged_with_the_public_relays() {
        let args = daemon_args(&Relays::n0());
        assert_eq!(args, ["daemon"]);
        // Mêmes fichiers qu'avant l'option : une mise à jour ne touche pas un service existant
        assert!(
            systemd_unit("/bin/tallylight", &args).contains("\nExecStart=/bin/tallylight daemon\n")
        );
        assert!(
            launch_agent_plist("dev.tallylight.daemon", "/bin/tallylight", &args, "/l.log")
                .contains("<array><string>/bin/tallylight</string><string>daemon</string></array>")
        );
        assert_eq!(
            relay_in_service(&systemd_unit("/bin/tallylight", &args)),
            None
        );
    }

    #[test]
    fn the_relay_is_kept_in_both_kinds_of_service() {
        let relays: Relays = "https://relais.exemple.fr,http://127.0.0.1:3340"
            .parse()
            .unwrap();
        let args = daemon_args(&relays);
        assert_eq!(
            args,
            [
                "daemon",
                "--relay",
                "https://relais.exemple.fr/,http://127.0.0.1:3340/"
            ]
        );

        let unit = systemd_unit("/home/moi/.local/bin/tallylight", &args);
        assert!(unit.contains(
            "\nExecStart=/home/moi/.local/bin/tallylight daemon --relay https://relais.exemple.fr/,http://127.0.0.1:3340/\n"
        ));
        assert_eq!(relay_in_service(&unit), Some(relays.clone()));

        let plist = launch_agent_plist(
            "dev.tallylight.daemon",
            "/Users/moi/A&B/tallylight",
            &args,
            "/Users/moi/Library/Logs/tallylight.log",
        );
        assert!(plist.contains(
            "<string>daemon</string><string>--relay</string><string>https://relais.exemple.fr/,http://127.0.0.1:3340/</string>"
        ));
        // Un chemin avec un caractère réservé reste un XML valide
        assert!(plist.contains("<string>/Users/moi/A&amp;B/tallylight</string>"));
        assert_eq!(relay_in_service(&plist), Some(relays));
    }

    #[test]
    fn a_service_without_a_readable_relay_falls_back_to_none() {
        assert_eq!(relay_in_service(""), None);
        assert_eq!(
            relay_in_service("ExecStart=/bin/tallylight daemon --relay"),
            None
        );
        assert_eq!(
            relay_in_service("ExecStart=/bin/tallylight daemon --relay pas-une-adresse"),
            None
        );
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
