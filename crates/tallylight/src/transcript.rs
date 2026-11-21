//! Le modèle utilisé par une session, lu dans sa transcription.
//!
//! Claude Code écrit chaque session dans `~/.claude/projects/<dossier encodé>/<session>.jsonl`, une
//! ligne JSON par message ; les réponses de l'agent portent `message.model` (« claude-opus-5-5 »).
//! Ce format est interne à Claude Code et peut changer : on n'en lit que ce champ, et une ligne
//! illisible est ignorée. Seule la fin du fichier est lue, la transcription pouvant peser des Mo.

use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::Value;

/// Taille lue en fin de transcription : assez pour contenir la dernière réponse de l'agent.
const TAIL: u64 = 256 * 1024;

/// Chemin de la transcription d'une session, tel que Claude Code le construit : chaque caractère du
/// dossier de travail qui n'est ni une lettre ni un chiffre devient `-`.
pub fn path(cwd: &str, session_id: &str) -> Option<PathBuf> {
    if cwd.is_empty() || session_id.is_empty() || session_id.contains(['/', '\\']) {
        return None;
    }
    let folder: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(
        crate::claude_dir()
            .ok()?
            .join("projects")
            .join(folder)
            .join(format!("{session_id}.jsonl")),
    )
}

/// Transcription d'une session : le chemin calculé, sinon une recherche du fichier de la session
/// dans les dossiers de projets (si l'encodage du dossier changeait d'une version à l'autre).
pub fn find(cwd: &str, session_id: &str) -> Option<PathBuf> {
    let expected = path(cwd, session_id)?;
    if expected.exists() {
        return Some(expected);
    }
    let name = format!("{session_id}.jsonl");
    std::fs::read_dir(expected.parent()?.parent()?)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(&name))
        .find(|candidate| candidate.exists())
}

/// Dernier modèle qui a répondu dans la session principale (les sous-agents sont ignorés).
pub fn last_model(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    // Lecture commencée au milieu d'une ligne : la première est incomplète
    if start > 0 {
        lines.next();
    }
    lines.rev().find_map(model_of)
}

fn model_of(line: &str) -> Option<String> {
    // Tri rapide avant de décoder la ligne entière
    if !line.contains("\"model\"") {
        return None;
    }
    let entry: Value = serde_json::from_str(line).ok()?;
    if entry["type"] != "assistant" || entry["isSidechain"] == true {
        return None;
    }
    let model = entry["message"]["model"].as_str()?;
    // Message fabriqué par Claude Code lui-même (erreur, interruption), pas par un modèle
    (!model.is_empty() && !model.starts_with('<')).then(|| model.to_string())
}

/// Évite de relire une transcription qui n'a pas changé depuis le dernier passage.
#[derive(Default)]
pub struct ModelCache {
    seen: HashMap<PathBuf, (SystemTime, Option<String>)>,
}

impl ModelCache {
    pub fn model(&mut self, cwd: &str, session_id: &str) -> Option<String> {
        let path = find(cwd, session_id)?;
        let modified = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()?;
        if let Some((at, model)) = self.seen.get(&path)
            && *at == modified
        {
            return model.clone();
        }
        let model = last_model(&path);
        self.seen.insert(path, (modified, model.clone()));
        model
    }

    /// Oublie les transcriptions des sessions qui ne sont plus là.
    pub fn retain(&mut self, keep: impl Fn(&Path) -> bool) {
        self.seen.retain(|path, _| keep(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_main_answer_gives_the_model() {
        let dir =
            std::env::temp_dir().join(format!("tallylight-transcript-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s.jsonl");
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"bonjour"}}"#,
            r#"{"type":"assistant","isSidechain":false,"message":{"model":"claude-sonnet-5-5","content":[]}}"#,
            r#"{"type":"assistant","isSidechain":false,"message":{"model":"claude-opus-5-5","content":[]}}"#,
            r#"{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku-5-5","content":[]}}"#,
            r#"{"type":"assistant","message":{"model":"<synthetic>","content":[]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"le \"model\" ici n'en est pas un"}}"#,
            "ligne illisible",
        ];
        std::fs::write(&file, lines.join("\n")).unwrap();
        assert_eq!(last_model(&file).as_deref(), Some("claude-opus-5-5"));

        // Seule la fin d'un gros fichier est lue
        let mut big = String::new();
        big.push_str(r#"{"type":"assistant","message":{"model":"claude-ancien-1"}}"#);
        big.push('\n');
        big.push_str(&"x".repeat(TAIL as usize + 10));
        big.push('\n');
        big.push_str(r#"{"type":"assistant","message":{"model":"claude-fable-5-2"}}"#);
        std::fs::write(&file, &big).unwrap();
        assert_eq!(last_model(&file).as_deref(), Some("claude-fable-5-2"));

        std::fs::write(&file, "").unwrap();
        assert_eq!(last_model(&file), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_transcript_folder_follows_claude_code() {
        let base = crate::claude_dir().unwrap().join("projects");
        assert_eq!(
            path("/Users/moi/DEV/DVP Launcher", "abc").unwrap(),
            base.join("-Users-moi-DEV-DVP-Launcher").join("abc.jsonl")
        );
        assert_eq!(
            path("/home/moi/.claude/x_y", "abc").unwrap(),
            base.join("-home-moi--claude-x-y").join("abc.jsonl")
        );
        assert_eq!(path("", "abc"), None);
        assert_eq!(path("/p", "../evil"), None);
    }
}
