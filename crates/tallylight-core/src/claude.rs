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
    #[serde(default)]
    pub notification_type: Option<String>,
    /// Présent quand le hook vient d'un sous-agent.
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// Effet d'un événement sur la session qui l'a émis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// La session passe dans cet état ; `tool` est l'outil en cours, s'il y en a un.
    Set { state: State, tool: Option<String> },
    /// La session apparaît ou reste telle quelle (on apprend seulement qu'elle existe).
    Seen,
    /// La session est fermée.
    Ended,
    /// Événement sans effet sur l'état.
    Ignored,
}

/// Décide ce qu'un événement change.
///
/// Limite connue : une interruption par l'utilisateur (Échap) ne déclenche aucun hook,
/// la session reste donc « au travail » jusqu'à l'événement suivant.
pub fn change(event: &HookEvent) -> Change {
    let set = |state, tool: Option<&String>| Change::Set {
        state,
        tool: tool.cloned(),
    };
    match event.hook_event_name.as_str() {
        "SessionStart" => Change::Seen,
        "UserPromptSubmit" => set(State::Working, None),
        "PreToolUse" => set(State::Working, event.tool_name.as_ref()),
        "PostToolUse" | "PostToolUseFailure" => set(State::Working, None),
        "PermissionRequest" => set(State::InputRequired, event.tool_name.as_ref()),
        "Notification" => match event.notification_type.as_deref() {
            Some("permission_prompt" | "elicitation_dialog" | "elicitation_url_dialog") => {
                set(State::InputRequired, None)
            }
            _ => Change::Ignored,
        },
        // Le tour d'un sous-agent se termine sans que la session ait fini
        "Stop" if event.agent_id.is_none() => set(State::Completed, None),
        "StopFailure" if event.agent_id.is_none() => set(State::Failed, None),
        "SessionEnd" => Change::Ended,
        _ => Change::Ignored,
    }
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
                tool: None
            }
        );
        let tool = event(r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"Bash"}"#);
        assert_eq!(
            change(&tool),
            Change::Set {
                state: State::Working,
                tool: Some("Bash".into())
            }
        );
        let stop = event(r#"{"session_id":"s","hook_event_name":"Stop"}"#);
        assert_eq!(
            change(&stop),
            Change::Set {
                state: State::Completed,
                tool: None
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
        assert!(matches!(
            change(&note),
            Change::Set {
                state: State::InputRequired,
                ..
            }
        ));
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
}
