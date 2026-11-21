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
use tallylight_client::Relays;
use tallylight_core::{
    Snapshot,
    claude::HookEvent,
    protocol::{ALPN, Hello, Welcome},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpListener,
    sync::watch,
};

use crate::{
    claude_local, data_dir,
    devices::{Admission, Device, Devices},
    identity, machine_name, now,
    store::Store,
    write_private,
};

struct App {
    store: Store,
    devices: Mutex<Devices>,
    /// Adresse de la machine pour les écrans ; vide tant que le réseau n'a pas été joint
    ticket: watch::Receiver<Option<String>>,
}

/// Temps laissé au réseau quand un appairage est demandé juste après le lancement.
const TICKET_PATIENCE: Duration = Duration::from_secs(15);

pub async fn run(port: u16, relays: Relays) -> Result<()> {
    // Le port local d'abord : suivre les sessions de la machine ne dépend pas du réseau.
    // Les hooks et les commandes locales n'arrivent que de la machine elle-même
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .with_context(|| format!("port {port} déjà pris ?"))?;
    println!("hooks attendus sur http://127.0.0.1:{port}/hooks/claude");
    if relays.is_n0() {
        println!(
            "relais : n0 (publics, réservés au développement ; --relay pour indiquer le sien)"
        );
    } else {
        println!("relais : {relays}");
    }

    // Le relais effectif part dans le ticket : `endpoint.addr()` annonce le relais joint
    let endpoint = relays
        .apply(Endpoint::builder(presets::N0))
        .secret_key(identity("key")?)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let (ticket_ready, ticket) = watch::channel(None);
    let app = Arc::new(App {
        store: Store::new(machine_name(), Some(data_dir()?.join("sessions.json"))),
        devices: Mutex::new(Devices::load(data_dir()?.join("devices.json"))),
        ticket,
    });

    // Le réseau ensuite, sans bloquer le reste : hors ligne, cette attente dure jusqu'au retour
    // du réseau, et seul l'appairage d'un nouvel écran en dépend
    let network = {
        let endpoint = endpoint.clone();
        let ticket_file = data_dir()?.join("ticket");
        async move {
            // Joint quand un relais a répondu : avec un relais injoignable, l'attente dure aussi
            endpoint.online().await;
            let address = endpoint.addr();
            let joined: Vec<String> = address.relay_urls().map(ToString::to_string).collect();
            let ticket = EndpointTicket::new(address).to_string();
            // Le ticket dit où joindre la machine ; il ne donne aucun droit sans appairage
            if let Err(error) = write_private(&ticket_file, ticket.as_bytes()) {
                println!("ticket non enregistré : {error:#}");
            }
            println!("réseau joint par {}, écrans attendus", joined.join(", "));
            let _ = ticket_ready.send(Some(ticket));
            // La tâche reste en vie : sa fin ne doit pas arrêter le démon
            std::future::pending::<()>().await
        }
    };
    let router = Router::new()
        .route("/hooks/claude", post(claude_hook))
        .route("/sessions", get(sessions))
        .route("/pair", post(pair))
        .route("/devices", get(devices))
        .route("/devices/{prefix}/remove", post(remove_device))
        .route("/name/reload", post(reload_name))
        .with_state(app.clone());

    let screens = {
        let endpoint = endpoint.clone();
        let app = app.clone();
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

    // Rapprochement régulier avec les sessions réellement vivantes sur la machine
    let reconcile = {
        let app = app.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            let mut models = crate::transcript::ModelCache::default();
            loop {
                tick.tick().await;
                let local = claude_local::read_with_models(&mut models);
                app.store.reconcile(&local, now());
            }
        }
    };

    tokio::select! {
        result = axum::serve(listener, router) => result?,
        _ = reconcile => {}
        _ = screens => {}
        _ = network => {}
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
    let Some(ticket) = ticket(app.ticket.clone(), TICKET_PATIENCE).await else {
        return Json(json!({
            "error": "le démon n'a pas encore joint le réseau (machine hors ligne, ou relais injoignable ?) : réessayer une fois la connexion revenue"
        }));
    };
    match app.devices.lock().unwrap().new_code() {
        Ok(code) => Json(json!({ "pairing": format!("{ticket}#{code}") })),
        Err(error) => Json(json!({ "error": error.to_string() })),
    }
}

/// Le ticket de la machine, en lui laissant `patience` pour arriver si le réseau n'est pas encore joint.
async fn ticket(mut ready: watch::Receiver<Option<String>>, patience: Duration) -> Option<String> {
    let known = tokio::time::timeout(patience, ready.wait_for(Option::is_some)).await;
    known.ok()?.ok()?.clone()
}

async fn devices(State(app): State<Arc<App>>) -> Json<Vec<Device>> {
    Json(app.devices.lock().unwrap().list().to_vec())
}

/// Reprend le nom choisi par `tallylight name`.
async fn reload_name(State(app): State<Arc<App>>) -> Json<Value> {
    let name = machine_name();
    app.store.rename(name.clone());
    Json(json!({ "name": name }))
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
    // Le chemin réseau de l'écran (relais ou direct, et vers quelle adresse) est journalisé à chaque
    // changement : c'est la seule façon de vérifier de quel côté passe réellement la connexion
    let mut route = String::new();
    let mut check = tokio::time::interval(Duration::from_secs(5));
    loop {
        let now = describe_route(connection);
        if now != route {
            println!("écran {} : {now}", hello.name);
            route = now;
        }
        let mut line = serde_json::to_vec(&*updates.borrow_and_update())?;
        line.push(b'\n');
        send.write_all(&line).await?;
        // Un tour d'horloge ne renvoie pas d'instantané : il ne sert qu'à relire le chemin
        loop {
            tokio::select! {
                changed = updates.changed() => { changed?; break; }
                _ = connection.closed() => return Ok(()),
                _ = check.tick() => {
                    let now = describe_route(connection);
                    if now != route {
                        println!("écran {} : {now}", hello.name);
                        route = now;
                    }
                }
            }
        }
    }
}

/// Chemin réseau utilisé à cet instant vers un écran.
fn describe_route(connection: &Connection) -> String {
    let paths = connection.paths();
    match paths.iter().find(|path| path.is_selected()) {
        // Sans le délai, qui varie sans cesse et noierait le journal
        Some(path) => format!(
            "{} via {:?}",
            if path.is_relay() { "relais" } else { "direct" },
            path.remote_addr()
        ),
        None => "aucun chemin".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pairing_waits_for_the_network_then_gives_up() {
        let (ready, ticket_slot) = watch::channel(None);
        // Hors ligne : la demande abandonne à l'échéance au lieu de rester bloquée
        assert_eq!(
            ticket(ticket_slot.clone(), Duration::from_millis(50)).await,
            None
        );
        // Le réseau arrive pendant l'attente : la demande aboutit
        let waiting = tokio::spawn(ticket(ticket_slot.clone(), Duration::from_secs(10)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        ready.send(Some("ticket".to_string())).unwrap();
        assert_eq!(waiting.await.unwrap().as_deref(), Some("ticket"));
        // Ensuite la réponse est immédiate
        assert_eq!(
            ticket(ticket_slot, Duration::ZERO).await.as_deref(),
            Some("ticket")
        );
    }
}
