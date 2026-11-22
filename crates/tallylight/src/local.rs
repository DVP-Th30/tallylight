//! Commandes qui parlent au démon de cette machine, par son port local.

use anyhow::{Context, Result, bail};
use qrcode::{QrCode, render::unicode};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use std::time::Duration;

use crate::local_port;

/// Requête HTTP minimale vers le démon local ; retourne le corps JSON de la réponse.
pub async fn request(method: &str, path: &str) -> Result<Value> {
    let mut stream = TcpStream::connect(("127.0.0.1", local_port()))
        .await
        .context(
            "the daemon on this machine is not answering: if it just started, try again in a few \
             seconds; otherwise run `tallylight install`, or `tallylight daemon` by hand",
        )?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .context("unreadable answer from the daemon")?;
    if !head.starts_with("HTTP/1.1 200") {
        bail!(
            "the daemon answered: {}",
            head.lines().next().unwrap_or_default()
        );
    }
    serde_json::from_str(body).context("unreadable answer from the daemon")
}

/// Temps laissé au démon pour recevoir un événement relayé : Codex n'accorde qu'une seconde à ses
/// hooks de fin de session et d'interruption.
const FORWARD_PATIENCE: Duration = Duration::from_millis(800);

/// Taille maximale d'un événement relayé.
const FORWARD_MAX: u64 = 8 * 1024 * 1024;

/// `tallylight hook <agent>` : relaie au démon l'événement qu'un agent sans hook `http` écrit sur
/// l'entrée de la commande. Ne dit rien et n'échoue jamais : un démon arrêté ou un événement
/// illisible ne doit ni ralentir l'agent ni afficher d'erreur dans sa session.
pub async fn forward(path: &str, port: u16) {
    use std::io::Read;
    let mut input = Vec::new();
    if std::io::stdin()
        .take(FORWARD_MAX)
        .read_to_end(&mut input)
        .is_err()
    {
        return;
    }
    let Some(body) = forwarded(&input) else {
        return;
    };
    let _ = tokio::time::timeout(FORWARD_PATIENCE, post(port, path, &body)).await;
}

/// L'événement tel qu'il part vers le démon : sans la sortie de l'outil, que le démon ne lit pas et
/// qui peut peser lourd.
fn forwarded(input: &[u8]) -> Option<Vec<u8>> {
    let mut event: Value = serde_json::from_slice(input).ok()?;
    event.as_object_mut()?.remove("tool_response");
    serde_json::to_vec(&event).ok()
}

async fn post(port: u16, path: &str, body: &[u8]) -> Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    // Attend la réponse : le démon a alors pris l'événement en compte
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(())
}

/// Demande au démon une chaîne d'appairage à usage unique.
pub async fn pairing() -> Result<String> {
    let answer = request("POST", "/pair").await?;
    answer["pairing"]
        .as_str()
        .map(str::to_string)
        .with_context(|| match answer["error"].as_str() {
            Some(reason) => format!("pairing failed: {reason}"),
            None => format!("pairing refused: {answer}"),
        })
}

pub async fn pair() -> Result<()> {
    let pairing = pairing().await?;
    let code = QrCode::new(pairing.as_bytes())?;
    let image = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();
    println!("{image}");
    println!("Valid for 5 minutes, for one screen. Without a camera:\n");
    println!("  tallylight watch '{pairing}'");
    Ok(())
}

pub async fn devices() -> Result<()> {
    let devices = request("GET", "/devices").await?;
    let devices = devices.as_array().cloned().unwrap_or_default();
    if devices.is_empty() {
        println!("no paired screen");
    }
    for device in devices {
        println!(
            "{}  {}",
            &device["id"].as_str().unwrap_or_default()[..10],
            device["name"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

pub async fn unpair(prefix: &str) -> Result<()> {
    let answer = request("POST", &format!("/devices/{prefix}/remove")).await?;
    let removed = answer["removed"].as_array().cloned().unwrap_or_default();
    if removed.is_empty() {
        bail!("no screen whose identity starts with \"{prefix}\"");
    }
    for device in removed {
        println!("removed: {}", device["name"].as_str().unwrap_or_default());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forwarded_event_leaves_the_tool_output_behind() {
        let body = forwarded(
            br#"{"session_id":"s","hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"cat secrets"},"tool_response":"top secret"}"#,
        )
        .unwrap();
        let event: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(event["hook_event_name"], "PostToolUse");
        assert_eq!(event["tool_input"]["command"], "cat secrets");
        assert!(event.get("tool_response").is_none());
        // Entrée illisible : rien n'est envoyé
        assert_eq!(forwarded(b"not json"), None);
        assert_eq!(forwarded(b"[1,2]"), None);
        assert_eq!(forwarded(b""), None);
    }

    #[tokio::test]
    async fn an_event_is_posted_whole_and_a_closed_port_fails_at_once() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // L'en-tête et le corps peuvent arriver en plusieurs morceaux
            let mut received = Vec::new();
            while !received.ends_with(b"}") {
                let mut chunk = [0; 1024];
                let read = stream.read(&mut chunk).await.unwrap();
                assert!(read > 0, "the request stopped before its body");
                received.extend_from_slice(&chunk[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            stream.shutdown().await.unwrap();
            String::from_utf8_lossy(&received).to_string()
        });
        post(port, "/hooks/codex", br#"{"a":1}"#).await.unwrap();
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /hooks/codex HTTP/1.1\r\n"));
        assert!(request.ends_with("\r\n\r\n{\"a\":1}"));
        // Plus personne n'écoute : l'envoi échoue tout de suite, sans attendre
        assert!(post(port, "/hooks/codex", b"{}").await.is_err());
    }
}
