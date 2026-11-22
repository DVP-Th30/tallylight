//! Commandes qui parlent au démon de cette machine, par son port local.

use anyhow::{Context, Result, bail};
use qrcode::{QrCode, render::unicode};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

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
