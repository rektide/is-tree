use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn get_beads_prefix(path: &Path) -> Option<String> {
    let beads_dir = path.join(".beads");
    if !beads_dir.exists() {
        return None;
    }

    let issues_path = beads_dir.join("issues.jsonl");
    if !issues_path.exists() {
        return None;
    }

    let file = File::open(&issues_path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;

    if first_line.trim().is_empty() {
        return None;
    }

    let parsed: serde_json::Value = serde_json::from_str(&first_line).ok()?;
    let id = parsed.get("id")?.as_str()?;

    let prefix = id.rsplit_once('-').map(|(p, _)| p)?;
    if prefix.is_empty() {
        return None;
    }

    Some(prefix.to_string())
}
