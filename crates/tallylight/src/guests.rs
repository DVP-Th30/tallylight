//! Machines invitées : une machine éphémère (session hébergée en ligne, conteneur) envoie l'état
//! de ses sessions à une machine d'accueil, qui le rediffuse à ses écrans. Les deux côtés de la
//! connexion sont ici. Voir `docs/DECISIONS.md` (D21).

use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use iroh::{Endpoint, EndpointAddr, endpoint::Connection};
use iroh_tickets::endpoint::EndpointTicket;
use tallylight_core::{
    Session, Snapshot,
    protocol::{FORWARD_ALPN, Join, Welcome},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::{invites::SECRET_PREFIX, machine_name, store::Store};

/// Variable d'environnement qui porte l'invitation quand `--forward` n'est pas donné.
pub const FORWARD_ENV: &str = "TALLYLIGHT_FORWARD";

/// Taille maximale d'une ligne reçue d'une invitée.
const LINE_MAX: u64 = 256 * 1024;

/// Sessions retenues par invitée.
const SESSIONS_MAX: usize = 50;

/// Temps minimal entre deux instantanés pris en compte d'une même invitée.
const PACE: Duration = Duration::from_millis(100);

/// Où envoyer ses sessions : la machine d'accueil et le secret de l'invitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    address: EndpointAddr,
    secret: String,
}

impl FromStr for Target {
    type Err = anyhow::Error;

    /// Lit la chaîne « ticket#secret » affichée par `tallylight invite`.
    fn from_str(text: &str) -> Result<Self> {
        let (ticket, secret) = text
            .trim()
            .rsplit_once('#')
            .context("not an invitation: expected the string printed by `tallylight invite`")?;
        if !secret.starts_with(SECRET_PREFIX) {
            bail!(
                "this is a pairing string for a screen, not an invitation: run `tallylight invite <name>` on the machine that should receive the sessions"
            );
        }
        let ticket = EndpointTicket::from_str(ticket)
            .ok()
            .context("not an invitation: unreadable machine address")?;
        Ok(Self {
            address: ticket.endpoint_addr().clone(),
            secret: secret.to_string(),
        })
    }
}

/// Côté invitée : envoie les sessions de cette machine à la machine d'accueil, et recommence
/// tant que le démon tourne. Ne rend jamais la main.
pub async fn forward(endpoint: Endpoint, store: Arc<Store>, target: Target) {
    let mut delay = Duration::from_secs(2);
    loop {
        let started = tokio::time::Instant::now();
        match forward_once(&endpoint, &store, &target).await {
            Ok(Ended::Closed) => println!("forward: connection closed, trying again"),
            Ok(Ended::Refused) => {
                println!(
                    "forward: invitation refused (revoked, or too many machines use it); trying again in a minute"
                );
                delay = Duration::from_secs(60);
            }
            Err(error) => println!("forward: {error:#}; trying again"),
        }
        // Une connexion qui a tenu remet l'attente à zéro
        if started.elapsed() > Duration::from_secs(60) {
            delay = Duration::from_secs(2);
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(30));
    }
}

enum Ended {
    Closed,
    Refused,
}

async fn forward_once(endpoint: &Endpoint, store: &Store, target: &Target) -> Result<Ended> {
    let connection = endpoint
        .connect(target.address.clone(), FORWARD_ALPN)
        .await
        .context("could not reach the receiving machine")?;
    let (mut send, recv) = connection.open_bi().await?;
    let mut line = serde_json::to_vec(&Join {
        name: machine_name(),
        invite: target.secret.clone(),
    })?;
    line.push(b'\n');
    send.write_all(&line).await?;
    let welcome = BufReader::new(recv)
        .lines()
        .next_line()
        .await?
        .context("the receiving machine closed the connection")?;
    let welcome: Welcome = serde_json::from_str(&welcome).context("unreadable answer")?;
    if !welcome.ok {
        return Ok(Ended::Refused);
    }
    println!("forward: sessions of this machine are sent to the receiving machine");

    let mut updates = store.subscribe();
    let mut sent: Option<Vec<Session>> = None;
    loop {
        // Seulement les sessions de cette machine : pas de renvoi en chaîne de ses propres invitées
        let own = own_sessions(&updates.borrow_and_update());
        if sent.as_ref() != Some(&own) {
            let mut line = serde_json::to_vec(&Snapshot {
                machine: machine_name(),
                sessions: own.clone(),
            })?;
            line.push(b'\n');
            send.write_all(&line).await?;
            sent = Some(own);
        }
        tokio::select! {
            changed = updates.changed() => changed?,
            _ = connection.closed() => return Ok(Ended::Closed),
        }
    }
}

fn own_sessions(snapshot: &Snapshot) -> Vec<Session> {
    snapshot
        .sessions
        .iter()
        .filter(|session| session.host.is_none())
        .cloned()
        .collect()
}

/// Côté accueil : vérifie l'invitation, puis remplace les sessions de l'invitée à chaque
/// instantané reçu. `check` rend le nom de l'invitation qui porte un secret.
pub async fn serve_guest(
    connection: &Connection,
    store: &Store,
    check: impl Fn(&str) -> Option<String>,
) -> Result<()> {
    let (mut send, recv) = connection.accept_bi().await?;
    let mut reader = BufReader::new(recv);
    let join = tokio::time::timeout(Duration::from_secs(10), read_line(&mut reader))
        .await
        .context("the machine did not introduce itself")??
        .context("the machine did not introduce itself")?;
    let join: Join = serde_json::from_slice(&join).context("unreadable introduction")?;

    let admitted = check(&join.invite).and_then(|invite| store.guest_join(&invite));
    let mut line = serde_json::to_vec(&Welcome {
        ok: admitted.is_some(),
    })?;
    line.push(b'\n');
    send.write_all(&line).await?;
    let Some((id, host)) = admitted else {
        send.finish()?;
        // Laisse le refus arriver avant de fermer
        let _ = tokio::time::timeout(Duration::from_secs(2), connection.closed()).await;
        bail!("refused (invitation unknown, revoked or full)");
    };
    println!("guest machine joined: {host}");

    let result = receive(&mut reader, connection, store, id, &host).await;
    store.guest_leave(id);
    println!("guest machine left: {host}");
    result
}

async fn receive(
    reader: &mut BufReader<iroh::endpoint::RecvStream>,
    connection: &Connection,
    store: &Store,
    id: u64,
    host: &str,
) -> Result<()> {
    let mut check = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            line = read_line(reader) => {
                let Some(line) = line? else {
                    return Ok(());
                };
                let snapshot: Snapshot = serde_json::from_slice(&line).context("unreadable snapshot")?;
                let sessions = snapshot
                    .sessions
                    .into_iter()
                    .take(SESSIONS_MAX)
                    .map(|session| trusted(session, host))
                    .collect();
                if !store.guest_update(id, sessions) {
                    bail!("invitation revoked");
                }
                tokio::time::sleep(PACE).await;
            }
            _ = connection.closed() => return Ok(()),
            // Une invitation révoquée ferme la connexion même si l'invitée n'envoie plus rien
            _ = check.tick() => {
                if !store.guest_admitted(id) {
                    bail!("invitation revoked");
                }
            }
        }
    }
}

/// Une ligne, sans son retour à la ligne ; `None` à la fin du flux. Une ligne trop longue est une erreur.
async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let read = reader
        .take(LINE_MAX + 1)
        .read_until(b'\n', &mut line)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if line.len() as u64 > LINE_MAX {
        bail!("line too long");
    }
    Ok(Some(line))
}

/// Une session reçue d'une invitée, ramenée à ce que la machine d'accueil accepte de rediffuser :
/// une invitée n'est pas de confiance. Le nom de sa machine est imposé, les textes sont bornés,
/// et seul un lien `https://` est gardé.
fn trusted(session: Session, host: &str) -> Session {
    Session {
        id: short(&session.id, 100),
        agent: short(&session.agent, 40),
        cwd: short(&session.cwd, 400),
        title: session.title.map(|text| short(&text, 140)),
        state: session.state,
        tool: session.tool.map(|text| short(&text, 60)),
        detail: session.detail.map(|text| short(&text, 140)),
        last_message: session.last_message.map(|text| short(&text, 601)),
        url: session
            .url
            .filter(|url| url.starts_with("https://") && url.len() <= 500)
            .filter(|url| !url.chars().any(|c| c.is_control() || c.is_whitespace())),
        model: session.model.map(|text| short(&text, 80)),
        background_tasks: session.background_tasks,
        host: Some(host.to_string()),
        updated_at: session.updated_at,
    }
}

/// Les `max` premiers caractères, sans caractère de contrôle.
fn short(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tallylight_core::State;

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            agent: "claude-code".into(),
            cwd: "/home/user/app".into(),
            title: None,
            state: Some(State::Working),
            tool: None,
            detail: None,
            last_message: None,
            url: None,
            model: None,
            background_tasks: 0,
            host: None,
            updated_at: 7,
        }
    }

    #[test]
    fn a_guest_session_is_cut_down_to_what_screens_may_show() {
        let mut raw = session("s");
        raw.host = Some("my-server".into());
        raw.title = Some("x".repeat(500));
        raw.detail = Some("line one\nline two".into());
        raw.url = Some("javascript:alert(1)".into());
        let clean = trusted(raw, "cloud");
        // Le nom de machine annoncé par l'invitée est ignoré
        assert_eq!(clean.host.as_deref(), Some("cloud"));
        assert_eq!(clean.title.unwrap().len(), 140);
        assert_eq!(clean.detail.as_deref(), Some("line oneline two"));
        assert_eq!(clean.url, None);
        assert_eq!(clean.state, Some(State::Working));
        assert_eq!(clean.updated_at, 7);

        let mut linked = session("t");
        linked.url = Some("https://claude.ai/code/session_1".into());
        assert_eq!(
            trusted(linked, "cloud").url.as_deref(),
            Some("https://claude.ai/code/session_1")
        );
        let mut spaced = session("u");
        spaced.url = Some("https://a.example/ b".into());
        assert_eq!(trusted(spaced, "cloud").url, None);
    }

    #[test]
    fn an_invitation_string_is_told_apart_from_a_pairing_string() {
        assert!("nothing".parse::<Target>().is_err());
        let pairing = "endpointabc#0123456789";
        let error = pairing.parse::<Target>().unwrap_err().to_string();
        assert!(error.contains("pairing string"), "{error}");
        let error = "junk#inv00".parse::<Target>().unwrap_err().to_string();
        assert!(error.contains("unreadable machine address"), "{error}");
    }

    #[tokio::test]
    async fn a_line_longer_than_the_limit_is_refused() {
        let mut short_input = BufReader::new(&b"{\"a\":1}\nrest"[..]);
        assert_eq!(
            read_line(&mut short_input).await.unwrap().as_deref(),
            Some(&b"{\"a\":1}\n"[..])
        );
        let long = vec![b'x'; LINE_MAX as usize + 10];
        let mut long_input = BufReader::new(&long[..]);
        assert!(read_line(&mut long_input).await.is_err());
        let mut empty = BufReader::new(&b""[..]);
        assert_eq!(read_line(&mut empty).await.unwrap(), None);
    }

    #[test]
    fn guests_add_their_sessions_and_take_them_away() {
        let store = Store::new("home".into(), None);
        let (first, name) = store.guest_join("cloud").unwrap();
        assert_eq!(name, "cloud");
        let (second, name) = store.guest_join("cloud").unwrap();
        assert_eq!(name, "cloud 2");
        assert_eq!(store.guests_count("cloud"), 2);

        assert!(store.guest_update(first, vec![trusted(session("a"), "cloud")]));
        assert!(store.guest_update(second, vec![trusted(session("b"), "cloud 2")]));
        let shown = store.snapshot();
        assert_eq!(shown.machine, "home");
        let hosts: Vec<_> = shown
            .sessions
            .iter()
            .map(|s| s.host.clone().unwrap())
            .collect();
        assert_eq!(hosts, ["cloud", "cloud 2"]);
        // Ce que l'invitée renverrait à son tour : rien de ce qui vient d'ailleurs
        assert!(own_sessions(&shown).is_empty());

        // Le nom libéré est repris par la suivante
        store.guest_leave(first);
        assert_eq!(store.snapshot().sessions.len(), 1);
        assert_eq!(store.guest_join("cloud").unwrap().1, "cloud");

        // Révocation : tout le monde sort, et une invitée sortie ne peut plus rien envoyer
        store.guests_revoke("cloud");
        assert!(store.snapshot().sessions.is_empty());
        assert!(!store.guest_admitted(second));
        assert!(!store.guest_update(second, vec![session("b")]));
    }
}
