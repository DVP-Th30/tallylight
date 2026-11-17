//! Premier écran : l'état des sessions dans le terminal, redessiné à chaque changement.

use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use iroh::{Endpoint, endpoint::presets};
use iroh_tickets::endpoint::EndpointTicket;
use tallylight_core::{Session, Snapshot, State};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{ALPN, data_dir};

pub async fn run(ticket: Option<&str>) -> Result<()> {
    let ticket = match ticket {
        Some(ticket) => ticket.to_string(),
        None => std::fs::read_to_string(data_dir()?.join("ticket"))
            .context("aucun démon local : lancer `tallylight daemon`, ou donner un ticket")?,
    };
    let ticket = EndpointTicket::from_str(ticket.trim()).context("ticket illisible")?;

    let endpoint = Endpoint::bind(presets::N0).await?;
    let connection = endpoint
        .connect(ticket.endpoint_addr().clone(), ALPN)
        .await?;
    let recv = connection.accept_uni().await?;
    let mut lines = BufReader::new(recv).lines();
    // Hors d'un terminal (sortie redirigée), on n'efface pas l'écran : un bloc par instantané
    let redraw = std::io::IsTerminal::is_terminal(&std::io::stdout());
    while let Some(line) = lines.next_line().await? {
        let snapshot: Snapshot = serde_json::from_str(&line).context("instantané illisible")?;
        if redraw {
            print!("\x1b[2J\x1b[H");
        }
        print!("{}", render(&snapshot, now()));
    }
    endpoint.close().await;
    Ok(())
}

/// Une ligne par session, celles qui attendent l'utilisateur en premier.
fn render(snapshot: &Snapshot, now: u64) -> String {
    let mut sessions: Vec<&Session> = snapshot.sessions.iter().collect();
    sessions.sort_by_key(|session| (rank(session.state), session.project().to_string()));
    let mut out = format!("{} · {} session(s)\n", snapshot.machine, sessions.len());
    for session in sessions {
        let (mark, label) = match session.state {
            Some(State::InputRequired) => ("!", "attend une réponse"),
            Some(State::Working) => ("●", "au travail"),
            Some(State::Completed) => ("✓", "fini"),
            Some(State::Failed) => ("✗", "erreur"),
            Some(State::Canceled) => ("–", "interrompu"),
            None => ("·", "ouverte"),
        };
        let tool = session
            .tool
            .as_deref()
            .map(|tool| format!(" ({tool})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {mark} {:<24} {label}{tool} · {}\n",
            session.project(),
            ago(now.saturating_sub(session.updated_at)),
        ));
    }
    out
}

fn rank(state: Option<State>) -> u8 {
    match state {
        Some(State::InputRequired) => 0,
        Some(State::Failed) => 1,
        Some(State::Working) => 2,
        Some(State::Completed) | Some(State::Canceled) => 3,
        None => 4,
    }
}

fn ago(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min", seconds / 60),
        _ => format!("{} h", seconds / 3600),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(cwd: &str, state: Option<State>, tool: Option<&str>, updated_at: u64) -> Session {
        Session {
            id: cwd.into(),
            agent: "claude-code".into(),
            cwd: cwd.into(),
            state,
            tool: tool.map(Into::into),
            updated_at,
        }
    }

    #[test]
    fn sessions_waiting_for_the_user_come_first() {
        let snapshot = Snapshot {
            machine: "serveurdvp".into(),
            sessions: vec![
                session("/p/alpha", Some(State::Working), Some("Bash"), 995),
                session("/p/beta", Some(State::InputRequired), None, 880),
                session("/p/gamma", Some(State::Completed), None, 0),
            ],
        };
        let lines: Vec<String> = render(&snapshot, 1000).lines().map(String::from).collect();
        assert_eq!(lines[0], "serveurdvp · 3 session(s)");
        assert!(lines[1].contains("beta") && lines[1].ends_with("attend une réponse · 2 min"));
        assert!(lines[2].contains("alpha") && lines[2].ends_with("au travail (Bash) · 5 s"));
        assert!(lines[3].contains("gamma") && lines[3].ends_with("fini · 16 min"));
    }
}
