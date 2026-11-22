//! tallylight : suit les sessions des agents de code d'une machine et diffuse leur état aux écrans appairés.
//!
//!     tallylight daemon [--port N] [--relay R]   # reçoit les hooks, tient l'état, sert les écrans
//!     tallylight watch [--relay R] [appairage]   # affiche l'état ; sans argument, celui de cette machine
//!     tallylight pair                # affiche un QR et une chaîne d'appairage pour un nouvel écran
//!     tallylight devices             # liste les écrans appairés
//!     tallylight unpair <identité>   # retire un écran (début de son identité)
//!     tallylight install [--no-service] [--relay R]  # branche Claude Code sur le démon, lancé en permanence
//!                                    # (systemd, launchd, ou tâche planifiée sous Windows)
//!     tallylight uninstall           # retire les hooks et le service
//!     tallylight name [nom]          # nom de la machine sur les écrans (« Mac » plutôt que le nom système)
//!     tallylight hooks [--port N]    # affiche les hooks, pour les ajouter à la main
//!
//! `R` : relais iroh, `n0` (relais publics de l'éditeur d'iroh, le défaut) ou une ou plusieurs
//! adresses `https://…` séparées par des virgules. Sans `--relay`, `TALLYLIGHT_RELAY` s'applique.

mod claude_local;
mod daemon;
mod devices;
mod hooks;
mod install;
mod local;
mod store;
mod transcript;
mod watch;
#[cfg(windows)]
mod windows;

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use iroh::SecretKey;
use tallylight_client::Relays;

/// Port local où le démon reçoit les hooks et les commandes.
pub const DEFAULT_PORT: u16 = 47700;

/// Variable d'environnement qui choisit les relais quand `--relay` n'est pas donné.
pub const RELAY_ENV: &str = "TALLYLIGHT_RELAY";

/// Renvoi vers l'aide, ajouté aux erreurs d'usage.
const HINT: &str = "run `tallylight --help` for usage";

/// Aide affichée par `--help`, et par `tallylight` sans argument.
const HELP: &str = "\
tallylight: the state of your coding agent sessions, on the screens you pair

Usage: tallylight <command> [options]

Commands:
  install [--no-service] [--relay R]  add the Claude Code hooks and run the daemon as a service
  uninstall                           remove the hooks and the service (data is kept)
  pair                                print a QR code and a one-time pairing string for a new screen
  watch [--relay R] [pairing]         follow this machine and every paired machine in the terminal
  devices                             list the paired screens
  unpair <id>                         remove a screen (start of its identity)
  name [name | --reset]               show or choose the machine name shown on screens
  daemon [--port N] [--relay R]       run the daemon in the foreground
  hooks [--port N]                    print the hooks, to add them by hand
  help                                show this help

Options:
  --port N       local port of the daemon (default 47700)
  --relay R      iroh relay: `n0` (public relays, the default) or one or more https:// URLs
                 separated by commas
  -h, --help     show this help
  -V, --version  show the version

Environment:
  TALLYLIGHT_RELAY  relay used when --relay is not given";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some((&command, rest)) = args.split_first() else {
        println!("{HELP}");
        return Ok(());
    };
    match command {
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        "--version" | "-V" => {
            println!("tallylight {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "daemon" => {
            let options = Options::parse(rest, DAEMON_OPTIONS, 0)?;
            #[cfg(windows)]
            if options.background {
                windows::detach_console(&daemon_log()?)?;
            }
            daemon::run(options.port(), options.relays()?).await
        }
        "watch" => {
            let options = Options::parse(rest, &["--relay"], 1)?;
            watch::run(options.free.first().copied(), &options.relays()?).await
        }
        "pair" if rest.is_empty() => local::pair().await,
        "devices" if rest.is_empty() => local::devices().await,
        "unpair" if rest.len() == 1 => local::unpair(rest[0]).await,
        "install" => {
            let options = Options::parse(rest, &["--no-service", "--relay"], 0)?;
            install::install(!options.no_service, options.relay_choice()?)
        }
        "uninstall" if rest.is_empty() => install::uninstall(),
        "name" => name(rest).await,
        "hooks" => {
            let options = Options::parse(rest, &["--port"], 0)?;
            println!("{}", hooks::claude_settings(options.port()));
            Ok(())
        }
        // Commande connue, mais arguments en trop ou manquants
        "pair" | "devices" | "uninstall" => bail!("`{command}` takes no argument\n{HINT}"),
        "unpair" => bail!("`unpair` takes the start of one screen identity\n{HINT}"),
        _ => bail!("unknown command: {command}\n{HINT}"),
    }
}

/// Options de `daemon`. `--background` (Windows seulement) : option interne de la tâche planifiée
/// écrite par `install`, qui détache le démon de sa console et journalise dans un fichier.
#[cfg(not(windows))]
const DAEMON_OPTIONS: &[&str] = &["--port", "--relay"];
#[cfg(windows)]
const DAEMON_OPTIONS: &[&str] = &["--port", "--relay", "--background"];

/// Journal du démon lancé par la tâche planifiée de Windows.
#[cfg(windows)]
pub fn daemon_log() -> Result<PathBuf> {
    Ok(data_dir()?.join("daemon.log"))
}

/// Options d'une commande, dans n'importe quel ordre.
#[derive(Debug, Default, PartialEq)]
struct Options<'a> {
    port: Option<u16>,
    relay: Option<&'a str>,
    no_service: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    background: bool,
    /// Arguments sans tiret (la chaîne d'appairage de `watch`).
    free: Vec<&'a str>,
}

impl<'a> Options<'a> {
    /// `allowed` : options acceptées par la commande ; `max_free` : arguments libres permis.
    fn parse(args: &[&'a str], allowed: &[&str], max_free: usize) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.iter().copied();
        while let Some(arg) = args.next() {
            if !arg.starts_with("--") {
                options.free.push(arg);
                continue;
            }
            if !allowed.contains(&arg) {
                bail!("unknown option: {arg}\n{HINT}");
            }
            let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
            match arg {
                "--port" => options.port = Some(value()?.parse().context("invalid port")?),
                "--relay" => options.relay = Some(value()?),
                "--no-service" => options.no_service = true,
                "--background" => options.background = true,
                _ => bail!("option {arg} is not handled"),
            }
        }
        if options.free.len() > max_free {
            bail!("unexpected argument: {}\n{HINT}", options.free.join(" "));
        }
        Ok(options)
    }

    fn port(&self) -> u16 {
        self.port.unwrap_or_else(local_port)
    }

    /// Relais demandés explicitement, par l'option ou par l'environnement ; `None` sinon.
    fn relay_choice(&self) -> Result<Option<Relays>> {
        let env = std::env::var(RELAY_ENV).ok();
        relay_choice(self.relay, env.as_deref())
    }

    /// Relais à utiliser : ceux demandés, sinon les relais publics de n0.
    fn relays(&self) -> Result<Relays> {
        Ok(self.relay_choice()?.unwrap_or_default())
    }
}

/// L'option l'emporte sur la variable d'environnement ; une variable vide compte pour absente.
fn relay_choice(option: Option<&str>, env: Option<&str>) -> Result<Option<Relays>> {
    let (text, source) = match (option, env) {
        (Some(text), _) => (text, "--relay"),
        (None, Some(text)) if !text.trim().is_empty() => (text, RELAY_ENV),
        _ => return Ok(None),
    };
    text.parse()
        .map(Some)
        .with_context(|| format!("{source}: \"{text}\""))
}

/// Port du démon de cette machine ; `TALLYLIGHT_PORT` le remplace (essais, second démon).
pub fn local_port() -> u16 {
    std::env::var("TALLYLIGHT_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Dossier personnel de l'utilisateur : `HOME`, ou `%USERPROFILE%` sous Windows.
pub fn home_dir() -> Result<PathBuf> {
    #[cfg(not(windows))]
    const VARIABLE: &str = "HOME";
    #[cfg(windows)]
    const VARIABLE: &str = "USERPROFILE";
    std::env::var_os(VARIABLE)
        .map(PathBuf::from)
        .with_context(|| format!("{VARIABLE} is not set"))
}

/// Dossier de configuration de Claude Code : `~/.claude`, soit `%USERPROFILE%\.claude` sous
/// Windows (https://code.claude.com/docs/en/settings, lu le 2026-11-22), sauf `CLAUDE_CONFIG_DIR`.
pub fn claude_dir() -> Result<PathBuf> {
    Ok(match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => home_dir()?.join(".claude"),
    })
}

/// Nom de la machine, tel qu'un écran l'affiche : celui choisi par `tallylight name`, sinon le nom
/// système (« serveurdvp », « Th30 »).
pub fn machine_name() -> String {
    chosen_name().unwrap_or_else(system_name)
}

/// Nom choisi par `tallylight name`, s'il y en a un.
fn chosen_name() -> Option<String> {
    let name = std::fs::read_to_string(data_dir().ok()?.join("name")).ok()?;
    clean_name(&name)
}

fn system_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        // Windows donne le nom de la machine dans l'environnement
        .or_else(|| std::env::var("COMPUTERNAME").ok().filter(|_| cfg!(windows)))
        .or_else(|| {
            // macOS n'a pas ce fichier
            let output = std::process::Command::new("hostname").output().ok()?;
            String::from_utf8(output.stdout).ok()
        })
        .map(|name| name.trim().trim_end_matches(".local").to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "machine".to_string())
}

/// Un nom affichable : une ligne, sans caractère de contrôle, 40 caractères au plus.
fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(40)
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// `tallylight name` : affiche le nom ; `name <nom>` le choisit ; `name --reset` revient au nom système.
/// Le démon le reprend aussitôt, sans redémarrer.
async fn name(args: &[&str]) -> Result<()> {
    let path = data_dir()?.join("name");
    match args {
        [] => {
            match chosen_name() {
                Some(name) => println!("{name} (system name: {})", system_name()),
                None => println!(
                    "{} (system name; run `tallylight name <name>` to choose another)",
                    system_name()
                ),
            }
            return Ok(());
        }
        ["--reset"] => {
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        words => {
            let Some(name) = clean_name(&words.join(" ")) else {
                bail!("the name is empty");
            };
            write_private(&path, name.as_bytes())?;
        }
    }
    let name = machine_name();
    match local::request("POST", "/name/reload").await {
        Ok(_) => println!("machine name: {name}"),
        Err(_) => {
            println!("machine name: {name} (applied the next time the daemon starts)")
        }
    }
    Ok(())
}

/// Dossier des données de la machine : identités réseau, ticket, appareils appairés.
/// `~/.local/share/tallylight`, ou `%LOCALAPPDATA%\tallylight` sous Windows ; `XDG_DATA_HOME`
/// l'emporte partout (essais, second démon).
pub fn data_dir() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => default_data_base()?,
    };
    let dir = base.join("tallylight");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(not(windows))]
fn default_data_base() -> Result<PathBuf> {
    Ok(home_dir()?.join(".local/share"))
}

/// Données propres à cette machine et à cet utilisateur, hors du profil itinérant.
#[cfg(windows)]
fn default_data_base() -> Result<PathBuf> {
    Ok(match std::env::var_os("LOCALAPPDATA") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home_dir()?.join("AppData").join("Local"),
    })
}

/// Identité réseau conservée dans le fichier `name`, créée au premier usage : un cœur garde le même
/// ticket d'un lancement à l'autre, un écran reste reconnu après son appairage.
pub fn identity(name: &str) -> Result<SecretKey> {
    let path = data_dir()?.join(name);
    if let Ok(bytes) = std::fs::read(&path) {
        let bytes: [u8; 32] = bytes
            .try_into()
            .ok()
            .context("the identity key is unreadable")?;
        return Ok(SecretKey::from_bytes(&bytes));
    }
    let key = SecretKey::generate();
    write_private(&path, &key.to_bytes())?;
    Ok(key)
}

/// Écrit un fichier réservé à l'utilisateur, d'un coup : fichier temporaire créé directement en
/// 0600 (la clé n'est jamais lisible par d'autres, même un instant), puis renommage. Un démon
/// arrêté en pleine écriture laisse donc l'ancien contenu intact, jamais un fichier tronqué.
/// Sous Windows il n'y a pas de mode à poser : le fichier hérite des droits de son dossier, et
/// `%LOCALAPPDATA%` n'est ouvert qu'à l'utilisateur, au système et aux administrateurs (D16).
pub fn write_private(path: &Path, content: &[u8]) -> Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("could not write {}", temporary.display()))?;
    #[cfg(unix)]
    {
        // Fichier temporaire resté d'un essai précédent avec d'autres droits
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    std::io::Write::write_all(&mut file, content)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)
        .with_context(|| format!("could not replace {}", path.display()))?;
    Ok(())
}

/// Secondes depuis 1970.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_name_is_one_short_clean_line() {
        assert_eq!(clean_name("  Mac \n").as_deref(), Some("Mac"));
        assert_eq!(
            clean_name("Mac\tdu\u{7}salon").as_deref(),
            Some("Macdusalon")
        );
        assert_eq!(clean_name(&"x".repeat(60)).map(|name| name.len()), Some(40));
        assert_eq!(clean_name(" \n "), None);
    }

    #[test]
    fn private_files_are_replaced_whole_and_stay_private() {
        let dir = std::env::temp_dir().join(format!("tallylight-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("devices.json");
        write_private(&path, b"premier contenu, plus long").unwrap();
        write_private(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!dir.join("devices.json.tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_are_read_in_any_order() {
        let daemon = ["--port", "--relay"];
        let options = Options::parse(
            &["--relay", "https://r.example", "--port", "4242"],
            &daemon,
            0,
        )
        .unwrap();
        assert_eq!(options.port, Some(4242));
        assert_eq!(options.relay, Some("https://r.example"));

        let options = Options::parse(&["ticket#code", "--relay", "n0"], &["--relay"], 1).unwrap();
        assert_eq!(options.free, ["ticket#code"]);
        assert_eq!(options.relay, Some("n0"));

        let options = Options::parse(&["--no-service"], &["--no-service", "--relay"], 0).unwrap();
        assert!(options.no_service && options.relay.is_none());
    }

    #[test]
    fn unknown_missing_or_extra_arguments_are_refused() {
        assert!(Options::parse(&["--relay"], &["--relay"], 0).is_err());
        assert!(Options::parse(&["--relay", "x"], &["--port"], 0).is_err());
        assert!(Options::parse(&["--port", "abc"], &["--port"], 0).is_err());
        assert!(Options::parse(&["a", "b"], &["--relay"], 1).is_err());
        assert!(Options::parse(&["a"], &["--port"], 0).is_err());
    }

    #[test]
    fn the_option_wins_over_the_environment() {
        let own = |text: &str| Some(text.parse::<Relays>().unwrap());
        assert_eq!(relay_choice(None, None).unwrap(), None);
        // Variable vide : comme absente
        assert_eq!(relay_choice(None, Some(" ")).unwrap(), None);
        assert_eq!(
            relay_choice(None, Some("https://env.example")).unwrap(),
            own("https://env.example")
        );
        assert_eq!(
            relay_choice(Some("https://option.example"), Some("https://env.example")).unwrap(),
            own("https://option.example")
        );
        // `--relay n0` revient explicitement aux relais publics, malgré l'environnement
        assert_eq!(
            relay_choice(Some("n0"), Some("https://env.example")).unwrap(),
            Some(Relays::n0())
        );
        let error = relay_choice(None, Some("pas une adresse")).unwrap_err();
        assert!(format!("{error:#}").contains(RELAY_ENV));
    }
}
