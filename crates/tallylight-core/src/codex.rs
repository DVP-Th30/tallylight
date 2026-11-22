//! Traduction des hooks de Codex (OpenAI) en changements d'état.
//!
//! Codex reprend le format des hooks de Claude Code : mêmes champs (`session_id`, `cwd`,
//! `hook_event_name`, `tool_name`, `tool_input`, `last_assistant_message`), plus `model`. Ce qui
//! diffère : pas de hook `http` (une commande reçoit l'événement sur son entrée), un événement
//! `Interrupt`, pas de `StopFailure` ni de `Notification`, et d'autres noms d'outils.
//! Références lues le 2026-11-22 : https://developers.openai.com/codex/hooks et le dépôt
//! openai/codex (`codex-rs/hooks/src/schema.rs`, version 0.171.0). Voir `docs/DECISIONS.md` (D17).

use crate::{
    State,
    claude::{Change, HookEvent, one_line, program},
};

/// Décide ce qu'un événement de Codex change.
///
/// Aucun événement ne signale un tour qui échoue (erreur du modèle ou du réseau) : `failed` n'est
/// donc jamais produit pour Codex.
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
        "PostToolUse" => idle(State::Working),
        "PermissionRequest" => with_tool(State::InputRequired),
        // La fin du tour d'un sous-agent est un autre événement (`SubagentStop`), ignoré plus bas
        "Stop" => idle(State::Completed),
        // Émis seulement pour le fil principal, quand l'utilisateur interrompt un tour en cours
        "Interrupt" => idle(State::Canceled),
        "SessionEnd" => Change::Ended,
        _ => Change::Ignored,
    }
}

/// Résumé court de ce que fait un outil de Codex. Même règle que pour Claude Code : jamais le
/// contenu d'un fichier ni les arguments d'une commande.
fn summary(event: &HookEvent) -> Option<String> {
    let input = event.tool_input.as_ref()?;
    let text = |key: &str| input.get(key).and_then(|value| value.as_str());
    let found = match event.tool_name.as_deref()? {
        // `description` n'accompagne que certaines demandes d'autorisation
        "Bash" => text("description").or_else(|| program(text("command")?)),
        // `command` contient le correctif entier : on n'en sort que le nom du premier fichier
        "apply_patch" => patched_file(text("command")?),
        _ => None,
    }?;
    one_line(found)
}

/// Nom du premier fichier touché par un correctif au format de Codex (`*** Update File: chemin`).
fn patched_file(patch: &str) -> Option<&str> {
    patch.lines().find_map(|line| {
        let path = ["*** Add File: ", "*** Update File: ", "*** Delete File: "]
            .iter()
            .find_map(|marker| line.strip_prefix(marker))?
            .trim();
        Some(crate::last_component(path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::last_message;

    fn event(json: &str) -> HookEvent {
        serde_json::from_str(json).unwrap()
    }

    // Corps construits champ pour champ d'après les structures `*CommandInput` de
    // `codex-rs/hooks/src/schema.rs` (openai/codex, commit a0995c2, 2026-11-22) ; les valeurs
    // (identifiants, chemins, nom du modèle) sont des exemples.
    const PROMPT: &str = r#"{"session_id":"019ab2f4-6c1e-7a10-9f3e-2b1d4c5e6f70","turn_id":"t1",
        "transcript_path":"/home/me/.codex/sessions/2026/11/22/rollout-x.jsonl","cwd":"/home/me/api",
        "hook_event_name":"UserPromptSubmit","model":"gpt-example","permission_mode":"default",
        "prompt":"fix the tests"}"#;
    const PRE_TOOL: &str = r#"{"session_id":"s","turn_id":"t1","transcript_path":null,"cwd":"/home/me/api",
        "hook_event_name":"PreToolUse","model":"gpt-example","permission_mode":"default",
        "tool_name":"Bash","tool_input":{"command":"/usr/bin/curl -H 'Authorization: Bearer abc' https://x"},
        "tool_use_id":"call_1"}"#;
    const POST_TOOL: &str = r#"{"session_id":"s","turn_id":"t1","transcript_path":null,"cwd":"/home/me/api",
        "hook_event_name":"PostToolUse","model":"gpt-example","permission_mode":"default",
        "tool_name":"Bash","tool_input":{"command":"echo hello"},"tool_response":"hello\n",
        "tool_use_id":"call_1"}"#;
    const PERMISSION: &str = r#"{"session_id":"s","turn_id":"t1","transcript_path":null,"cwd":"/home/me/api",
        "hook_event_name":"PermissionRequest","model":"gpt-example","permission_mode":"default",
        "tool_name":"Bash","tool_input":{"command":"rm -rf build","description":"Clean the build folder"}}"#;
    const STOP: &str = r#"{"session_id":"s","turn_id":"t1","transcript_path":null,"cwd":"/home/me/api",
        "hook_event_name":"Stop","model":"gpt-example","permission_mode":"default",
        "stop_hook_active":false,"last_assistant_message":"Tests pass."}"#;
    const INTERRUPT: &str = r#"{"session_id":"s","turn_id":"t1","transcript_path":null,"cwd":"/home/me/api",
        "hook_event_name":"Interrupt","model":"gpt-example","permission_mode":"default"}"#;
    const SESSION_END: &str = r#"{"session_id":"s","transcript_path":null,"cwd":"/home/me/api",
        "hook_event_name":"SessionEnd","reason":"other"}"#;

    fn set(state: State, tool: Option<&str>, detail: Option<&str>) -> Change {
        Change::Set {
            state,
            tool: tool.map(str::to_string),
            detail: detail.map(str::to_string),
        }
    }

    #[test]
    fn a_turn_goes_working_then_completed() {
        let prompt = event(PROMPT);
        assert_eq!(prompt.model.as_deref(), Some("gpt-example"));
        assert_eq!(change(&prompt), set(State::Working, None, None));
        // Sans description, seul le nom du programme sort : jamais les arguments
        assert_eq!(
            change(&event(PRE_TOOL)),
            set(State::Working, Some("Bash"), Some("curl"))
        );
        assert_eq!(change(&event(POST_TOOL)), set(State::Working, None, None));
        let stop = event(STOP);
        assert_eq!(change(&stop), set(State::Completed, None, None));
        assert_eq!(last_message(&stop).as_deref(), Some("Tests pass."));
    }

    #[test]
    fn a_permission_request_needs_the_user() {
        assert_eq!(
            change(&event(PERMISSION)),
            set(
                State::InputRequired,
                Some("Bash"),
                Some("Clean the build folder")
            )
        );
        // `description` peut être nulle : on retombe sur le nom du programme
        let bare = event(
            r#"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Bash",
                "tool_input":{"command":"rm -rf build","description":null}}"#,
        );
        assert_eq!(
            change(&bare),
            set(State::InputRequired, Some("Bash"), Some("rm"))
        );
    }

    #[test]
    fn an_interrupt_cancels_and_the_end_closes() {
        assert_eq!(change(&event(INTERRUPT)), set(State::Canceled, None, None));
        assert_eq!(change(&event(SESSION_END)), Change::Ended);
    }

    #[test]
    fn a_patch_is_reduced_to_its_first_file_name() {
        let patch = event(
            r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"apply_patch",
                "tool_input":{"command":"*** Begin Patch\n*** Update File: src/billing/invoice.rs\n@@\n-secret\n+other\n*** Add File: notes.md\n+x\n*** End Patch"}}"#,
        );
        assert_eq!(
            change(&patch),
            set(State::Working, Some("apply_patch"), Some("invoice.rs"))
        );
        // Un outil MCP ou inconnu : son nom, sans détail tiré de ses arguments
        let mcp = event(
            r#"{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"mcp__filesystem__read_file",
                "tool_input":{"path":"/etc/passwd"}}"#,
        );
        assert_eq!(
            change(&mcp),
            set(State::Working, Some("mcp__filesystem__read_file"), None)
        );
    }

    #[test]
    fn subagent_and_bookkeeping_events_change_nothing() {
        for name in [
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
            "PreModelRequest",
            "PostModelRequest",
        ] {
            let other = event(&format!(
                r#"{{"session_id":"s","hook_event_name":"{name}","agent_id":"a1","last_assistant_message":"x"}}"#
            ));
            assert_eq!(change(&other), Change::Ignored);
            assert_eq!(last_message(&other), None);
        }
        let start = event(
            r#"{"session_id":"s","transcript_path":null,"cwd":"/p","hook_event_name":"SessionStart",
                "model":"gpt-example","permission_mode":"default","source":"startup"}"#,
        );
        assert_eq!(change(&start), Change::Seen);
    }
}
