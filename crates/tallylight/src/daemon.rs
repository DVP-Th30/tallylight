//! Le cœur : reçoit les hooks en HTTP local, tient l'état, le pousse aux écrans autorisés par iroh.

use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use iroh::{
    Endpoint,
    endpoint::{Connection, presets},
};
use iroh_tickets::endpoint::EndpointTicket;
use serde_json::{Value, json};
use tallylight_core::{
    Snapshot,
    claude::HookEvent,
    protocol::{ALPN, Hello, Welcome},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpListener,
};

use crate::{
    data_dir,
    devices::{Admission, Device, Devices},
    identity, now,
    store::Store,
    write_private,
};

struct App {
    store: Store,
    devices: Mutex<Devices>,
    ticket: String,
}

pub async fn run(port: u16) -> Result<()> {
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(identity("key")?)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    endpoint.online().await;
    let ticket = EndpointTicket::new(endpoint.addr()).to_string();
    // Le ticket dit où joindre la machine ; il ne donne aucun droit sans appairage
    write_private(&data_dir()?.join("ticket"), ticket.as_bytes())?;

    let app = Arc::new(App {
        store: Store::new(machine_name()),
        devices: Mutex::new(Devices::load(data_dir()?.join("devices.json"))),
        ticket,
    });

    // Les hooks et les commandes locales n'arrivent que de la machine elle-même
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| format!("port {port} déjà pris ?"))?;
    println!("hooks attendus sur http://127.0.0.1:{port}/hooks/claude");
    let router = Router::new()
        .route("/hooks/claude", post(claude_hook))
        .route("/sessions", get(sessions))
        .route("/pair", post(pair))
        .route("/devices", get(devices))
        .route("/devices/{prefix}/remove", post(remove_device))
        .with_state(app.clone());

    let screens = {
        let endpoint = endpoint.clone();
        async move {
            while let Some(incoming) = endpoint.accept().await {
                let app = app.clone();
                tokio::spawn(async move {
                    let Ok(connection) = incoming.await else {
                        return;
                    };
                    let peer = connection.remote_id().fmt_short();
                    match serve_screen(&connection, &app).await {
                        Ok(()) => println!("écran déconnecté : {peer}"),
                        Err(error) => println!("écran {peer} : {error:#}"),
                    }
                });
            }
        }
    };

    tokio::select! {
        result = axum::serve(listener, router) => result?,
        _ = screens => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    endpoint.close().await;
    Ok(())
}

/// Réponse vide : le hook ne décide de rien, il informe seulement.
async fn claude_hook(State(app): State<Arc<App>>, Json(body): Json<Value>) -> Json<Value> {
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
        app.store.apply_claude(&event);
    }
    Json(json!({}))
}

async fn sessions(State(app): State<Arc<App>>) -> Json<Snapshot> {
    Json(app.store.snapshot())
}

/// Chaîne d'appairage à montrer à un écran : le ticket et un code à usage unique.
async fn pair(State(app): State<Arc<App>>) -> Json<Value> {
    match app.devices.lock().unwrap().new_code() {
        Ok(code) => Json(json!({ "pairing": format!("{}#{code}", app.ticket) })),
        Err(error) => Json(json!({ "error": error.to_string() })),
    }
}

async fn devices(State(app): State<Arc<App>>) -> Json<Vec<Device>> {
    Json(app.devices.lock().unwrap().list().to_vec())
}

async fn remove_device(State(app): State<Arc<App>>, Path(prefix): Path<String>) -> Json<Value> {
    match app.devices.lock().unwrap().remove(&prefix) {
        Ok(removed) => Json(json!({ "removed": removed })),
        Err(error) => Json(json!({ "error": error.to_string() })),
    }
}

/// Vérifie que l'écran est autorisé, puis lui envoie l'instantané courant et un nouveau à chaque
/// changement : une ligne JSON par instantané.
async fn serve_screen(connection: &Connection, app: &App) -> Result<()> {
    // L'identité de l'écran est prouvée par la connexion chiffrée, pas par ce qu'il déclare
    let id = connection.remote_id().to_string();
    let (mut send, recv) = connection.accept_bi().await?;
    let mut lines = BufReader::new(recv).lines();
    let hello = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .context("l'écran ne s'est pas présenté")??
        .context("l'écran ne s'est pas présenté")?;
    let hello: Hello = serde_json::from_str(&hello).context("présentation illisible")?;

    let admission =
        app.devices
            .lock()
            .unwrap()
            .admit(&id, &hello.name, hello.code.as_deref(), now())?;
    let welcome = Welcome {
        ok: admission != Admission::Refused,
    };
    let mut line = serde_json::to_vec(&welcome)?;
    line.push(b'\n');
    send.write_all(&line).await?;
    match admission {
        Admission::Refused => {
            send.finish()?;
            // Laisse le refus arriver avant de fermer
            let _ = tokio::time::timeout(Duration::from_secs(2), connection.closed()).await;
            bail!("refusé (« {} », non appairé)", hello.name);
        }
        Admission::Paired => println!("écran appairé : {} ({})", hello.name, &id[..10]),
        Admission::Known => println!("écran connecté : {} ({})", hello.name, &id[..10]),
    }

    let mut updates = app.store.subscribe();
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

fn machine_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "machine".to_string())
}
