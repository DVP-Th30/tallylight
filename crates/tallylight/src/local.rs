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
            "le démon de cette machine ne répond pas : s'il vient d'être lancé, réessayer dans \
             quelques secondes ; sinon `tallylight install`, ou `tallylight daemon` à la main",
        )?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .context("réponse du démon illisible")?;
    if !head.starts_with("HTTP/1.1 200") {
        bail!(
            "le démon a répondu : {}",
            head.lines().next().unwrap_or_default()
        );
    }
    serde_json::from_str(body).context("réponse du démon illisible")
}

/// Demande au démon une chaîne d'appairage à usage unique.
pub async fn pairing() -> Result<String> {
    let answer = request("POST", "/pair").await?;
    answer["pairing"]
        .as_str()
        .map(str::to_string)
        .with_context(|| match answer["error"].as_str() {
            Some(reason) => format!("appairage impossible : {reason}"),
            None => format!("appairage refusé : {answer}"),
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
    println!("Valable 5 minutes, pour un seul écran. Sans caméra :\n");
    println!("  tallylight watch '{pairing}'");
    Ok(())
}

pub async fn devices() -> Result<()> {
    let devices = request("GET", "/devices").await?;
    let devices = devices.as_array().cloned().unwrap_or_default();
    if devices.is_empty() {
        println!("aucun écran appairé");
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
        bail!("aucun écran dont l'identité commence par « {prefix} »");
    }
    for device in removed {
        println!("retiré : {}", device["name"].as_str().unwrap_or_default());
    }
    Ok(())
}
