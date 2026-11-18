//! Bibliothèque d'écran : se connecter à un cœur tallylight et recevoir l'état de ses sessions.
//!
//! ```no_run
//! # async fn demo() -> Result<(), tallylight_client::Error> {
//! let key = tallylight_client::generate_key();            // à conserver : c'est l'identité de l'écran
//! let screen = tallylight_client::Screen::new(key, "mon écran").await?;
//! let mut watch = screen.watch("<chaîne affichée par `tallylight pair`>").await?;
//! while let Some(snapshot) = watch.next().await? {
//!     println!("{} session(s) sur {}", snapshot.sessions.len(), snapshot.machine);
//! }
//! # Ok(()) }
//! ```

use std::str::FromStr;

use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, RecvStream, presets},
};
use iroh_tickets::endpoint::EndpointTicket;
use tallylight_core::protocol::{ALPN, Hello, Welcome};
pub use tallylight_core::{Session, Snapshot, State};
use tokio::io::{AsyncBufReadExt, BufReader, Lines};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// La chaîne d'appairage ou le ticket ne se lit pas.
    #[error("chaîne d'appairage illisible")]
    BadPairing,
    /// Le cœur ne connaît pas cet écran : il faut une chaîne fraîche de `tallylight pair`.
    #[error("cet écran n'est pas appairé avec la machine")]
    NotPaired,
    /// Machine injoignable, connexion coupée, réponse inattendue.
    #[error("connexion impossible : {0}")]
    Connection(String),
}

fn connection<E: std::fmt::Display>(error: E) -> Error {
    Error::Connection(error.to_string())
}

/// Tire une identité d'écran. À conserver : un écran appairé est reconnu par elle.
pub fn generate_key() -> [u8; 32] {
    SecretKey::generate().to_bytes()
}

/// Sépare une chaîne d'appairage en ticket et code éventuel.
fn split(pairing: &str) -> (&str, Option<&str>) {
    match pairing.trim().split_once('#') {
        Some((ticket, code)) => (ticket, Some(code)),
        None => (pairing.trim(), None),
    }
}

/// Identité de la machine désignée par une chaîne d'appairage ou un ticket : stable d'une connexion
/// à l'autre, elle sert de clé pour reconnaître la machine.
pub fn machine_id(pairing: &str) -> Result<String, Error> {
    let (ticket, _) = split(pairing);
    let address = EndpointTicket::from_str(ticket).map_err(|_| Error::BadPairing)?;
    Ok(address.endpoint_addr().id.to_string())
}

/// Un écran, avec son identité réseau. Peut suivre plusieurs machines.
pub struct Screen {
    endpoint: Endpoint,
    name: String,
}

impl Screen {
    pub async fn new(key: [u8; 32], name: impl Into<String>) -> Result<Self, Error> {
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(SecretKey::from_bytes(&key))
            .bind()
            .await
            .map_err(connection)?;
        Ok(Self {
            endpoint,
            name: name.into(),
        })
    }

    /// Se connecte à une machine. `pairing` est la chaîne « ticket#code » de `tallylight pair`,
    /// ou le ticket seul d'une machine où cet écran est déjà appairé.
    pub async fn watch(&self, pairing: &str) -> Result<Watch, Error> {
        let (ticket, code) = split(pairing);
        let code = code.map(str::to_string);
        let address = EndpointTicket::from_str(ticket).map_err(|_| Error::BadPairing)?;
        let connection = self
            .endpoint
            .connect(address.endpoint_addr().clone(), ALPN)
            .await
            .map_err(connection)?;

        let (mut send, recv) = connection.open_bi().await.map_err(self::connection)?;
        let hello = Hello {
            name: self.name.clone(),
            code,
        };
        let mut line = serde_json::to_vec(&hello).map_err(self::connection)?;
        line.push(b'\n');
        send.write_all(&line).await.map_err(self::connection)?;

        let mut lines = BufReader::new(recv).lines();
        let welcome = lines
            .next_line()
            .await
            .map_err(self::connection)?
            .ok_or_else(|| Error::Connection("connexion fermée".into()))?;
        let welcome: Welcome = serde_json::from_str(&welcome).map_err(self::connection)?;
        if !welcome.ok {
            return Err(Error::NotPaired);
        }
        Ok(Watch {
            connection,
            lines,
            ticket: ticket.to_string(),
        })
    }

    /// Ferme proprement les connexions de l'écran.
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// Une machine suivie : ses instantanés arrivent à chaque changement.
pub struct Watch {
    connection: Connection,
    lines: Lines<BufReader<RecvStream>>,
    ticket: String,
}

impl Watch {
    /// Attend l'instantané suivant ; `None` quand la machine ferme la connexion.
    pub async fn next(&mut self) -> Result<Option<Snapshot>, Error> {
        match self.lines.next_line().await.map_err(connection)? {
            Some(line) => serde_json::from_str(&line).map(Some).map_err(connection),
            None => Ok(None),
        }
    }

    /// Ticket de la machine, sans code : à conserver pour s'y reconnecter plus tard.
    pub fn ticket(&self) -> &str {
        &self.ticket
    }

    /// `Some(true)` si la connexion passe par un relais, `Some(false)` si elle est directe,
    /// `None` tant qu'aucun chemin n'est établi.
    pub fn relayed(&self) -> Option<bool> {
        self.connection
            .paths()
            .iter()
            .find(|path| path.is_selected())
            .map(|path| path.is_relay())
    }
}
