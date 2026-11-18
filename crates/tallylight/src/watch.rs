//! Premier écran : l'état des sessions dans le terminal, redessiné à chaque changement.

use anyhow::{Result, bail};
use tallylight_client::{Screen, Session, Snapshot, State};

use crate::{data_dir, identity, local, machine_name, now, write_private};

/// `pairing` : chaîne donnée par `tallylight pair` (« ticket#code »), ou ticket seul d'une machine
/// déjà appairée. Sans argument, on suit cette machine : le démon local fournit lui-même l'appairage.
pub async fn run(pairing: Option<&str>) -> Result<()> {
    let pairing = match pairing {
        Some(pairing) => pairing.to_string(),
        None => local::pairing().await?,
    };
    // L'écran garde son identité : une fois appairé, il est reconnu sans code
    let screen = Screen::new(
        identity("screen-key")?.to_bytes(),
        format!("terminal@{}", machine_name()),
    )
    .await?;
    let mut watch = match screen.watch(&pairing).await {
        Err(tallylight_client::Error::NotPaired) => bail!(
            "cet écran n'est pas appairé avec la machine : lancer `tallylight pair` sur elle et utiliser la chaîne affichée"
        ),
        other => other?,
    };
    remember(watch.ticket())?;

    // Hors d'un terminal (sortie redirigée), on n'efface pas l'écran : un bloc par instantané
    let redraw = std::io::IsTerminal::is_terminal(&std::io::stdout());
    while let Some(snapshot) = watch.next().await? {
        if redraw {
            print!("\x1b[2J\x1b[H");
        }
        print!("{}", render(&snapshot, now()));
    }
    screen.close().await;
    Ok(())
}

/// Une ligne par session, celles qui attendent l'utilisateur en premier.
fn render(snapshot: &Snapshot, now: u64) -> String {
    let mut sessions: Vec<&Session> = snapshot.sessions.iter().collect();
    sessions.sort_by_key(|session| (rank(session.state), session.label().to_string()));
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
        // « (Bash : Compile le projet) », ou l'outil seul, ou rien
        let tool = match (session.tool.as_deref(), session.detail.as_deref()) {
            (Some(tool), Some(detail)) => format!(" ({tool} : {detail})"),
            (Some(tool), None) => format!(" ({tool})"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "  {mark} {:<24} {label}{tool} · {}\n",
            session.label(),
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

/// Retient le ticket d'une machine appairée (un par ligne), pour la retrouver plus tard.
fn remember(ticket: &str) -> Result<()> {
    let path = data_dir()?.join("machines");
    let known = std::fs::read_to_string(&path).unwrap_or_default();
    if known.lines().any(|line| line == ticket) {
        return Ok(());
    }
    write_private(&path, format!("{known}{ticket}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(cwd: &str, state: Option<State>, tool: Option<&str>, updated_at: u64) -> Session {
        Session {
            id: cwd.into(),
            agent: "claude-code".into(),
            cwd: cwd.into(),
            title: None,
            state,
            tool: tool.map(Into::into),
            detail: None,
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
