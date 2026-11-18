//! Traduction des hooks de Claude Code en changements d'état.
//! Référence : https://code.claude.com/docs/en/hooks (lue le 2026-11-17).

use serde::Deserialize;

use crate::State;

/// Corps JSON qu'un hook de Claude Code envoie. Seuls les champs utiles sont lus.
#[derive(Debug, Clone, Deserialize)]
pub struct HookEvent {
    pub session_id: String,
    pub hook_event_name: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    /// Arguments de l'outil : on n'en garde qu'un résumé court, voir [`summary`].
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default)]
    pub notification_type: Option<String>,
    /// Présent quand le hook vient d'un sous-agent.
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// Effet d'un événement sur la session qui l'a émis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// La session passe dans cet état ; `tool` est l'outil en cours et `detail` ce qu'il fait
    /// (ou ce pour quoi une autorisation est demandée), s'ils sont connus.
    Set {
        state: State,
        tool: Option<String>,
        detail: Option<String>,
    },
    /// La session attend l'utilisateur, sans autre précision : l'outil et le détail déjà connus restent.
    Waiting,
    /// La session apparaît ou reste telle quelle (on apprend seulement qu'elle existe).
    Seen,
    /// La session est fermée.
    Ended,
    /// Événement sans effet sur l'état.
    Ignored,
}

/// Décide ce qu'un événement change.
///
/// Une interruption par l'utilisateur (Échap) ne déclenche aucun hook : elle est déduite ailleurs,
/// des fiches de session de Claude Code.
pub fn change(event: &HookEvent) -> Change {
    let idle = |state| Change::Set {
        state,
        tool: None,
        detail: None,
    };
    let with_tool = |state| Change::Set {
        state,
        tool: event.tool_name.clone(),
        detail: summary(event),
    };
    match event.hook_event_name.as_str() {
        "SessionStart" => Change::Seen,
        "UserPromptSubmit" => idle(State::Working),
        "PreToolUse" => with_tool(State::Working),
        "PostToolUse" | "PostToolUseFailure" => idle(State::Working),
        "PermissionRequest" => with_tool(State::InputRequired),
        "Notification" => match event.notification_type.as_deref() {
            Some("permission_prompt" | "elicitation_dialog" | "elicitation_url_dialog") => {
                Change::Waiting
            }
            _ => Change::Ignored,
        },
        // Le tour d'un sous-agent se termine sans que la session ait fini
        "Stop" if event.agent_id.is_none() => idle(State::Completed),
        "StopFailure" if event.agent_id.is_none() => idle(State::Failed),
        "SessionEnd" => Change::Ended,
        _ => Change::Ignored,
    }
}

/// Longueur maximale d'un résumé envoyé aux écrans.
const SUMMARY_MAX: usize = 140;

/// Résumé court de ce que fait un outil, tiré de ses arguments : la description d'une commande, le
/// nom d'un fichier, la question posée. Jamais le contenu d'un fichier ni la sortie d'une commande.
fn summary(event: &HookEvent) -> Option<String> {
    let input = event.tool_input.as_ref()?;
    let text = |key: &str| input.get(key).and_then(|value| value.as_str());
    let file = |key: &str| text(key).map(|path| path.rsplit('/').next().unwrap_or(path));
    let found = match event.tool_name.as_deref()? {
        "Bash" => text("description").or(text("command")),
        "Read" | "Edit" | "Write" | "NotebookEdit" => file("file_path").or(file("notebook_path")),
        "Grep" | "Glob" => text("pattern"),
        "WebFetch" => text("url"),
        "WebSearch" => text("query"),
        "Agent" | "Task" => text("description"),
        "AskUserQuestion" => input
            .get("questions")
            .and_then(|questions| questions.get(0))
            .and_then(|question| question.get("question"))
            .and_then(|question| question.as_str()),
        "ExitPlanMode" => Some("plan à valider"),
        _ => None,
    }?;
    let line = found.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return None;
    }
    let mut short: String = line.chars().take(SUMMARY_MAX).collect();
    if short.len() < line.len() {
        short.push('…');
    }
    Some(short)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(json: &str) -> HookEvent {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_turn_goes_working_then_completed() {
        let prompt = event(r#"{"session_id":"s","hook_event_name":"UserPromptSubmit","cwd":"/p"}"#);
        assert_eq!(
            change(&prompt),
            Change::Set {
                state: State::Working,
                tool: None,
                detail: None
            }
        );
        let tool = event(r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"Bash"}"#);
        assert_eq!(
            change(&tool),
            Change::Set {
                state: State::Working,
                tool: Some("Bash".into()),
                detail: None
            }
        );
        let stop = event(r#"{"session_id":"s","hook_event_name":"Stop"}"#);
        assert_eq!(
            change(&stop),
            Change::Set {
                state: State::Completed,
                tool: None,
                detail: None
            }
        );
    }

    #[test]
    fn a_permission_request_needs_the_user() {
        let ask =
            event(r#"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Bash"}"#);
        assert!(matches!(
            change(&ask),
            Change::Set {
                state: State::InputRequired,
                ..
            }
        ));
        let note = event(
            r#"{"session_id":"s","hook_event_name":"Notification","notification_type":"permission_prompt"}"#,
        );
        assert_eq!(change(&note), Change::Waiting);
    }

    #[test]
    fn idle_notifications_and_subagent_stops_change_nothing() {
        let idle = event(
            r#"{"session_id":"s","hook_event_name":"Notification","notification_type":"idle_prompt"}"#,
        );
        assert_eq!(change(&idle), Change::Ignored);
        let sub = event(r#"{"session_id":"s","hook_event_name":"Stop","agent_id":"a1"}"#);
        assert_eq!(change(&sub), Change::Ignored);
        let unknown = event(r#"{"session_id":"s","hook_event_name":"PreCompact"}"#);
        assert_eq!(change(&unknown), Change::Ignored);
    }

    #[test]
    fn session_end_closes_the_session() {
        let end = event(r#"{"session_id":"s","hook_event_name":"SessionEnd"}"#);
        assert_eq!(change(&end), Change::Ended);
    }

    #[test]
    fn a_tool_call_says_what_it_does() {
        let bash = event(
            r#"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Bash",
                "tool_input":{"command":"rm -rf build\nls","description":"Nettoie le dossier de build"}}"#,
        );
        assert_eq!(
            change(&bash),
            Change::Set {
                state: State::InputRequired,
                tool: Some("Bash".into()),
                detail: Some("Nettoie le dossier de build".into())
            }
        );
        let read = event(
            r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{"file_path":"/p/src/main.rs"}}"#,
        );
        assert!(
            matches!(change(&read), Change::Set { detail: Some(detail), .. } if detail == "main.rs")
        );
        let ask = event(
            r#"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion",
                "tool_input":{"questions":[{"question":"Quel nom pour le projet ?"}]}}"#,
        );
        assert!(
            matches!(change(&ask), Change::Set { detail: Some(detail), .. } if detail == "Quel nom pour le projet ?")
        );
        let long = event(&format!(
            r#"{{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":"{}"}}}}"#,
            "x".repeat(400)
        ));
        assert!(
            matches!(change(&long), Change::Set { detail: Some(detail), .. } if detail.chars().count() == 141)
        );
    }
}
