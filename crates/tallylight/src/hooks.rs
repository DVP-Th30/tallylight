//! Hooks à installer dans les agents pour qu'ils préviennent le démon.
//!
//! Claude Code et Codex rangent leurs hooks de la même façon : un objet `hooks`, une liste de
//! groupes par événement, chaque groupe portant ses propres `hooks`. Seul le hook lui-même change :
//! une adresse `http` pour Claude Code, une commande pour Codex (qui n'a pas de hook `http`).

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

/// Événements de Codex suivis. `SessionStart` n'y est pas : sans fiche de session pour dire si le
/// processus vit encore, une session qui n'a jamais travaillé resterait affichée pour rien.
const CODEX_EVENTS: [&str; 7] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
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

fn is_claude(group: &Value, port: u16) -> bool {
    let url = claude_url(port);
    handlers(group).any(|hook| hook["url"] == url.as_str())
}

/// Ajoute nos hooks aux réglages, à côté de ceux qui existent. Retourne faux si tout y était déjà.
pub fn add_claude(settings: &mut Value, port: u16) -> bool {
    let ours = |group: &Value| is_claude(group, port);
    add(settings, &CLAUDE_EVENTS, &claude_group(port), ours, |_| {
        true
    })
}

/// Retire nos hooks, sans toucher à ceux des autres outils. Retourne faux s'il n'y en avait pas.
pub fn remove_claude(settings: &mut Value, port: u16) -> bool {
    remove(settings, |group| is_claude(group, port))
}

/// Fin de la commande de nos hooks Codex : c'est elle qui les fait reconnaître, où que soit le binaire.
fn codex_suffix(port: u16) -> String {
    format!(" hook codex --port {port}")
}

/// Commande lancée par Codex à chaque événement : elle reçoit l'événement sur son entrée et le
/// relaie au démon. `exe` : chemin du binaire tallylight.
fn codex_command(exe: &str, port: u16) -> String {
    format!("{}{}", shell_quote(exe), codex_suffix(port))
}

/// Protège un chemin pour le shell par lequel Codex lance ses hooks (`$SHELL -lc <commande>`).
fn shell_quote(text: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-+".contains(c);
    if !text.is_empty() && text.chars().all(plain) {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

/// Hook `command` de Codex. Même délai court que pour Claude Code ; `SessionEnd` et `Interrupt`
/// n'acceptent de toute façon pas plus de 3 secondes.
fn codex_group(exe: &str, port: u16) -> Value {
    json!({ "hooks": [{ "type": "command", "command": codex_command(exe, port), "timeout": 2 }] })
}

/// Contenu d'un `hooks.json` de Codex qui ne porterait que nos hooks, à fusionner à la main.
pub fn codex_settings(exe: &str, port: u16) -> String {
    let mut settings = json!({});
    add_codex(&mut settings, exe, port);
    serde_json::to_string_pretty(&settings).unwrap()
}

fn is_codex(group: &Value, port: u16) -> bool {
    let suffix = codex_suffix(port);
    handlers(group).any(|hook| {
        hook["command"]
            .as_str()
            .is_some_and(|command| command.ends_with(&suffix))
    })
}

/// Ajoute nos hooks au `hooks.json` de Codex. Un hook laissé par une installation précédente à un
/// autre emplacement du binaire est remplacé, pas doublé.
pub fn add_codex(settings: &mut Value, exe: &str, port: u16) -> bool {
    let group = codex_group(exe, port);
    let command = codex_command(exe, port);
    let ours = |group: &Value| is_codex(group, port);
    let current = |group: &Value| handlers(group).any(|hook| hook["command"] == command.as_str());
    add(settings, &CODEX_EVENTS, &group, ours, current)
}

pub fn remove_codex(settings: &mut Value, port: u16) -> bool {
    remove(settings, |group| is_codex(group, port))
}

/// Les hooks d'un groupe.
fn handlers(group: &Value) -> impl Iterator<Item = &Value> {
    group["hooks"].as_array().into_iter().flatten()
}

/// Ajoute `group` à chaque événement, à côté des groupes des autres outils. `ours` reconnaît nos
/// groupes ; `current` dit si l'un d'eux est encore le bon (sinon il est remplacé).
fn add(
    settings: &mut Value,
    events: &[&str],
    group: &Value,
    ours: impl Fn(&Value) -> bool,
    current: impl Fn(&Value) -> bool,
) -> bool {
    if !settings.is_object() {
        *settings = json!({});
    }
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    let mut changed = false;
    for event in events {
        let groups = &mut settings["hooks"][*event];
        if !groups.is_array() {
            *groups = json!([]);
        }
        let groups = groups.as_array_mut().unwrap();
        let before = groups.len();
        groups.retain(|group| !ours(group) || current(group));
        changed |= groups.len() != before;
        if !groups.iter().any(&ours) {
            groups.push(group.clone());
            changed = true;
        }
    }
    changed
}

/// Retire les groupes reconnus par `ours`, sans laisser ni liste vide ni bloc `hooks` vide.
fn remove(settings: &mut Value, ours: impl Fn(&Value) -> bool) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for groups in hooks.values_mut().filter_map(Value::as_array_mut) {
        let before = groups.len();
        groups.retain(|group| !ours(group));
        changed |= groups.len() != before;
    }
    if !changed {
        return false;
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    if hooks.is_empty() {
        settings.as_object_mut().unwrap().remove("hooks");
    }
    true
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

    #[test]
    fn codex_gets_a_command_for_every_event() {
        let settings: Value =
            serde_json::from_str(&codex_settings("/home/me/.local/bin/tallylight", 4242)).unwrap();
        // `hooks.json` refuse toute clé inconnue à la racine : rien d'autre que `hooks`
        assert_eq!(settings.as_object().unwrap().len(), 1);
        let hooks = settings["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), CODEX_EVENTS.len());
        for event in ["PermissionRequest", "Stop", "Interrupt", "SessionEnd"] {
            assert_eq!(
                hooks[event],
                json!([{ "hooks": [{
                    "type": "command",
                    "command": "/home/me/.local/bin/tallylight hook codex --port 4242",
                    "timeout": 2
                }] }])
            );
        }
    }

    #[test]
    fn codex_hooks_live_next_to_the_others_and_follow_the_binary() {
        let other = json!({ "matcher": "Bash", "hooks": [{ "type": "command", "command": "python3 check.py" }] });
        let original = json!({
            "description": "my hooks",
            "hooks": { "PreToolUse": [other.clone()], "SessionStart": [other.clone()] }
        });
        let mut settings = original.clone();
        assert!(add_codex(&mut settings, "/old/tallylight", 4242));
        assert_eq!(settings["description"], "my hooks");
        assert_eq!(settings["hooks"]["PreToolUse"][0], other);
        assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
        assert!(!add_codex(&mut settings, "/old/tallylight", 4242));

        // Binaire déplacé : la commande est remplacée, pas doublée
        assert!(add_codex(&mut settings, "/new place/tallylight", 4242));
        let groups = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[1]["hooks"][0]["command"],
            "'/new place/tallylight' hook codex --port 4242"
        );
        // Un autre port est une autre installation (démon d'essai) : elle n'est pas touchée
        assert!(!remove_codex(&mut settings, 4243));

        assert!(remove_codex(&mut settings, 4242));
        assert_eq!(settings, original);
        assert!(!remove_codex(&mut settings, 4242));
    }

    #[test]
    fn paths_are_quoted_for_the_shell() {
        assert_eq!(
            shell_quote("/usr/local/bin/tallylight"),
            "/usr/local/bin/tallylight"
        );
        assert_eq!(
            shell_quote("/Users/me/My Tools/tallylight"),
            "'/Users/me/My Tools/tallylight'"
        );
        assert_eq!(shell_quote("/tmp/it's/t"), r"'/tmp/it'\''s/t'");
        assert_eq!(shell_quote("/tmp/$HOME/t"), "'/tmp/$HOME/t'");
    }
}
