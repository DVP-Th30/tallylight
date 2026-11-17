//! Le cœur : reçoit les hooks en HTTP local, tient l'état, le pousse aux écrans par iroh.

use std::{net::Ipv4Addr, path::Path, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, presets},
};
use iroh_tickets::endpoint::EndpointTicket;
use serde_json::{Value, json};
use tallylight_core::{Snapshot, claude::HookEvent};
use tokio::net::TcpListener;

use crate::{ALPN, data_dir, store::Store};

pub async fn run(port: u16) -> Result<()> {
    let store = Arc::new(Store::new(machine_name()));

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(identity()?)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    endpoint.online().await;
    let ticket = EndpointTicket::new(endpoint.addr()).to_string();
    // Le ticket donne accès à l'état : comme la clé, il n'est lisible que par son propriétaire
    write_private(&data_dir()?.join("ticket"), ticket.as_bytes())?;
    println!("ticket d'appairage : {ticket}");

    // Les hooks n'arrivent que de la machine elle-même
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| format!("port {port} déjà pris ?"))?;
    println!("hooks attendus sur http://127.0.0.1:{port}/hooks/claude");
    let app = Router::new()
        .route("/hooks/claude", post(claude_hook))
        .route("/sessions", get(sessions))
        .with_state(store.clone());

    let screens = {
        let endpoint = endpoint.clone();
        async move {
            while let Some(incoming) = endpoint.accept().await {
                let store = store.clone();
                tokio::spawn(async move {
                    let Ok(connection) = incoming.await else {
                        return;
                    };
                    let peer = connection.remote_id().fmt_short();
                    println!("écran connecté : {peer}");
                    let _ = serve_screen(connection, &store).await;
                    println!("écran déconnecté : {peer}");
                });
            }
        }
    };

    tokio::select! {
        result = axum::serve(listener, app) => result?,
        _ = screens => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    endpoint.close().await;
    Ok(())
}

/// Réponse vide : le hook ne décide de rien, il informe seulement.
async fn claude_hook(State(store): State<Arc<Store>>, Json(body): Json<Value>) -> Json<Value> {
    if let Some(path) = std::env::var_os("TALLYLIGHT_LOG_EVENTS") {
        // Journal brut des événements, pour étudier ce que l'agent envoie réellement
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{body}");
        }
    }
    if let Ok(event) = serde_json::from_value::<HookEvent>(body) {
        store.apply_claude(&event);
    }
    Json(json!({}))
}

async fn sessions(State(store): State<Arc<Store>>) -> Json<Snapshot> {
    Json(store.snapshot())
}

/// Envoie l'instantané courant, puis un nouveau à chaque changement : une ligne JSON par instantané.
async fn serve_screen(connection: Connection, store: &Store) -> Result<()> {
    let mut updates = store.subscribe();
    let mut send = connection.open_uni().await?;
    loop {
        let mut line = serde_json::to_vec(&*updates.borrow_and_update())?;
        line.push(b'\n');
        send.write_all(&line).await?;
        tokio::select! {
            changed = updates.changed() => changed?,
            _ = connection.closed() => return Ok(()),
        }
    }
}

/// Identité réseau de la machine, créée au premier lancement : le ticket reste le même d'un lancement à l'autre.
fn identity() -> Result<SecretKey> {
    let path = data_dir()?.join("key");
    if let Ok(bytes) = std::fs::read(&path) {
        let bytes: [u8; 32] = bytes.try_into().ok().context("clé d'identité illisible")?;
        return Ok(SecretKey::from_bytes(&bytes));
    }
    let key = SecretKey::generate();
    write_private(&path, &key.to_bytes())?;
    Ok(key)
}

/// Écrit un fichier lisible par son seul propriétaire.
fn write_private(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn machine_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "machine".to_string())
}
