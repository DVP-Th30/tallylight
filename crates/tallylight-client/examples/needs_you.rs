//! Un écran minimal : se connecte à une machine et écrit une ligne quand un agent attend une réponse.
//!
//!     cargo run -p tallylight-client --example needs_you -- '<pairing string>'
//!
//! La première fois, donner la chaîne affichée par `tallylight pair`. Ensuite, sans argument : la clé
//! de l'écran et le ticket de la machine sont gardés dans `.tallylight-example/`.

use std::{collections::HashSet, fs, path::Path};

use tallylight_client::{Screen, generate_key};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = Path::new(".tallylight-example");
    fs::create_dir_all(dir)?;

    // La clé est l'identité de l'écran : la machine la reconnaît sans nouveau code
    let key: [u8; 32] = match fs::read(dir.join("key"))
        .ok()
        .and_then(|k| k.try_into().ok())
    {
        Some(key) => key,
        None => {
            let key = generate_key();
            fs::write(dir.join("key"), key)?;
            key
        }
    };
    let pairing = match std::env::args().nth(1) {
        Some(pairing) => pairing,
        None => fs::read_to_string(dir.join("ticket"))
            .map_err(|_| "first run: pass the pairing string printed by `tallylight pair`")?,
    };

    let screen = Screen::new(key, "needs_you example").await?;
    let mut watch = screen.watch(pairing.trim()).await?;
    // Le ticket suffit aux connexions suivantes : le code d'appairage ne sert qu'une fois
    fs::write(dir.join("ticket"), watch.ticket())?;

    // Sessions déjà signalées, pour n'écrire qu'au moment où un agent se met à attendre
    let mut waiting = HashSet::new();
    while let Some(snapshot) = watch.next().await? {
        let now: HashSet<String> = snapshot
            .sessions
            .iter()
            .filter(|session| session.state.is_some_and(|state| state.needs_user()))
            .map(|session| session.id.clone())
            .collect();
        for session in &snapshot.sessions {
            if now.contains(&session.id) && !waiting.contains(&session.id) {
                let detail = session.detail.as_deref().unwrap_or("waiting for you");
                println!(
                    "{} · {} needs you: {detail}",
                    snapshot.machine,
                    session.label()
                );
            }
        }
        waiting = now;
    }
    println!("the machine closed the connection");
    Ok(())
}
