//! tallylight : suit les sessions des agents de code d'une machine et diffuse leur état aux écrans appairés.
//!
//!     tallylight daemon [--port N] [--relay R]   # reçoit les hooks, tient l'état, sert les écrans
//!     tallylight watch [--relay R] [appairage]   # affiche l'état ; sans argument, celui de cette machine
//!     tallylight pair                # affiche un QR et une chaîne d'appairage pour un nouvel écran
//!     tallylight devices             # liste les écrans appairés
//!     tallylight unpair <identité>   # retire un écran (début de son identité)
//!     tallylight install [--no-service] [--relay R]  # branche Claude Code sur le démon, lancé en permanence
//!     tallylight uninstall           # retire les hooks et le service
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

const USAGE: &str = "usage : tallylight daemon [--port N] [--relay R] | watch [--relay R] [appairage] | pair | devices | unpair <identité> | install [--no-service] [--relay R] | uninstall | hooks [--port N]";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let Some((&command, rest)) = args.split_first() else {
        bail!(USAGE);
    };
    match command {
        "daemon" => {
            let options = Options::parse(rest, &["--port", "--relay"], 0)?;
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
        "hooks" => {
            let options = Options::parse(rest, &["--port"], 0)?;
            println!("{}", hooks::claude_settings(options.port()));
            Ok(())
        }
        _ => bail!(USAGE),
    }
}

/// Options d'une commande, dans n'importe quel ordre.
#[derive(Debug, Default, PartialEq)]
struct Options<'a> {
    port: Option<u16>,
    relay: Option<&'a str>,
    no_service: bool,
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
                bail!("option inconnue : {arg}\n{USAGE}");
            }
            let mut value = || {
                args.next()
                    .with_context(|| format!("{arg} attend une valeur"))
            };
            match arg {
                "--port" => options.port = Some(value()?.parse().context("port invalide")?),
                "--relay" => options.relay = Some(value()?),
                "--no-service" => options.no_service = true,
                _ => bail!("option {arg} non traitée"),
            }
        }
        if options.free.len() > max_free {
            bail!("argument en trop : {}\n{USAGE}", options.free.join(" "));
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
        .with_context(|| format!("{source} : « {text} »"))
}

/// Port du démon de cette machine ; `TALLYLIGHT_PORT` le remplace (essais, second démon).
pub fn local_port() -> u16 {
    std::env::var("TALLYLIGHT_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
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

/// Écrit un fichier réservé à l'utilisateur, d'un coup : fichier temporaire créé directement en
/// 0600 (la clé n'est jamais lisible par d'autres, même un instant), puis renommage. Un démon
/// arrêté en pleine écriture laisse donc l'ancien contenu intact, jamais un fichier tronqué.
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
        .with_context(|| format!("écriture de {}", temporary.display()))?;
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
        .with_context(|| format!("remplacement de {}", path.display()))?;
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
