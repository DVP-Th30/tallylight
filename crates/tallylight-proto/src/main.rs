//! Prototype jetable du premier jalon : deux machines s'appairent par un ticket et l'une pousse
//! un état de session à l'autre via iroh. Sert à valider le réseau (direct ou relais), pas le format.
//!
//!     tallylight-proto listen            # affiche un ticket, puis pousse des états aux pairs connectés
//!     tallylight-proto connect <ticket>  # se connecte et affiche les états reçus

use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result, bail};
use iroh::{
    Endpoint,
    endpoint::{Connection, presets},
};
use iroh_tickets::endpoint::EndpointTicket;
use tallylight_core::State;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Identifiant du protocole, échangé à l'ouverture de la connexion.
const ALPN: &[u8] = b"tallylight/proto/0";

/// États joués en boucle par `listen`, pour simuler une session d'agent.
const SCRIPT: [State; 4] = [
    State::Working,
    State::InputRequired,
    State::Working,
    State::Completed,
];

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["listen"] => listen().await,
        ["connect", ticket] => connect(ticket).await,
        _ => bail!("usage : tallylight-proto listen | connect <ticket>"),
    }
}

async fn listen() -> Result<()> {
    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    endpoint.online().await;
    println!("ticket : {}", EndpointTicket::new(endpoint.addr()));

    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(async move {
            if let Err(error) = serve(incoming.await?).await {
                eprintln!("pair déconnecté : {error:#}");
            }
            anyhow::Ok(())
        });
    }
    Ok(())
}

/// Pousse une ligne par changement d'état sur un flux à sens unique.
async fn serve(connection: Connection) -> Result<()> {
    println!("pair connecté : {}", connection.remote_id().fmt_short());
    let mut send = connection.open_uni().await?;
    for (turn, state) in SCRIPT.iter().cycle().enumerate() {
        let line = format!("demo-session {} {turn}\n", state.as_a2a());
        send.write_all(line.as_bytes()).await?;
        println!("envoyé : {} ({})", state.as_a2a(), route(&connection));
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    Ok(())
}

async fn connect(ticket: &str) -> Result<()> {
    let ticket = EndpointTicket::from_str(ticket).context("ticket illisible")?;
    let endpoint = Endpoint::bind(presets::N0).await?;
    let connection = endpoint
        .connect(ticket.endpoint_addr().clone(), ALPN)
        .await?;
    println!("connecté à {}", connection.remote_id().fmt_short());

    let recv = connection.accept_uni().await?;
    let mut lines = BufReader::new(recv).lines();
    while let Some(line) = lines.next_line().await? {
        println!("reçu : {line} ({})", route(&connection));
    }
    endpoint.close().await;
    Ok(())
}

/// Chemin utilisé à cet instant : direct entre les deux machines, ou par un relais.
fn route(connection: &Connection) -> String {
    let paths = connection.paths();
    match paths.iter().find(|path| path.is_selected()) {
        Some(path) if path.is_relay() => format!("relais, {} ms", path.rtt().as_millis()),
        Some(path) => format!("direct, {} ms", path.rtt().as_millis()),
        None => "aucun chemin".to_string(),
    }
}
