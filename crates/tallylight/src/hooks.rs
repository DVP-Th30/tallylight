//! Hooks à installer dans les agents pour qu'ils préviennent le démon.

use serde_json::{Value, json};

/// Événements de Claude Code suivis. Chacun envoie son corps JSON au démon par un hook `http`.
const CLAUDE_EVENTS: [&str; 9] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Bloc `hooks` à fusionner dans un fichier de réglages de Claude Code.
///
/// Le délai est court : le démon répond tout de suite, et un démon arrêté ne doit pas ralentir l'agent.
pub fn claude_settings(port: u16) -> String {
    let hook = json!({
        "type": "http",
        "url": format!("http://127.0.0.1:{port}/hooks/claude"),
        "timeout": 2,
    });
    let hooks: serde_json::Map<String, Value> = CLAUDE_EVENTS
        .iter()
        .map(|event| (event.to_string(), json!([{ "hooks": [hook] }])))
        .collect();
    serde_json::to_string_pretty(&json!({ "hooks": hooks })).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_posts_to_the_daemon() {
        let settings: Value = serde_json::from_str(&claude_settings(4242)).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), CLAUDE_EVENTS.len());
        assert_eq!(
            hooks["Stop"][0]["hooks"][0]["url"],
            "http://127.0.0.1:4242/hooks/claude"
        );
    }
}
