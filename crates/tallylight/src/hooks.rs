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

/// Adresse du démon dans un hook : sert aussi à reconnaître nos hooks parmi ceux des autres outils.
fn claude_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/hooks/claude")
}

/// Hook `http` vers le démon. Le délai est court : le démon répond tout de suite, et un démon
/// arrêté ne doit pas ralentir l'agent.
fn claude_group(port: u16) -> Value {
    json!({ "hooks": [{ "type": "http", "url": claude_url(port), "timeout": 2 }] })
}

/// Bloc `hooks` à fusionner à la main dans un fichier de réglages de Claude Code.
pub fn claude_settings(port: u16) -> String {
    let mut settings = json!({});
    add_claude(&mut settings, port);
    serde_json::to_string_pretty(&settings).unwrap()
}

fn is_ours(group: &Value, port: u16) -> bool {
    let url = claude_url(port);
    group["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(|hook| hook["url"] == url.as_str()))
}

/// Ajoute nos hooks aux réglages, à côté de ceux qui existent. Retourne faux si tout y était déjà.
pub fn add_claude(settings: &mut Value, port: u16) -> bool {
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    let mut changed = false;
    for event in CLAUDE_EVENTS {
        let groups = &mut settings["hooks"][event];
        if !groups.is_array() {
            *groups = json!([]);
        }
        let groups = groups.as_array_mut().unwrap();
        if !groups.iter().any(|group| is_ours(group, port)) {
            groups.push(claude_group(port));
            changed = true;
        }
    }
    changed
}

/// Retire nos hooks, sans toucher à ceux des autres outils. Retourne faux s'il n'y en avait pas.
pub fn remove_claude(settings: &mut Value, port: u16) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        let before = groups.len();
        groups.retain(|group| !is_ours(group, port));
        changed |= groups.len() != before;
    }
    // Ne laisse ni liste vide ni bloc `hooks` vide derrière soi
    hooks.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    if hooks.is_empty() {
        settings.as_object_mut().unwrap().remove("hooks");
    }
    changed
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

    #[test]
    fn our_hooks_live_next_to_the_others() {
        let other = json!({ "hooks": [{ "type": "command", "command": "vibe-island-hook" }] });
        let mut settings =
            json!({ "hooks": { "Stop": [other.clone()], "PreCompact": [other.clone()] } });
        assert!(add_claude(&mut settings, 4242));
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(settings["hooks"]["Stop"][0], other);
        assert!(!add_claude(&mut settings, 4242));

        assert!(remove_claude(&mut settings, 4242));
        assert_eq!(
            settings,
            json!({ "hooks": { "Stop": [other.clone()], "PreCompact": [other] } })
        );
        assert!(!remove_claude(&mut settings, 4242));
    }
}
