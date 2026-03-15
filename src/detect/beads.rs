use std::path::Path;
use std::process::Command;

pub fn get_beads_prefix(path: &Path) -> Option<String> {
    let beads_dir = path.join(".beads");
    if !beads_dir.exists() {
        return None;
    }

    let output = Command::new("bd")
        .current_dir(path)
        .args(["sql", "--json"])
        .arg("SELECT value FROM config WHERE `key` = 'issue_prefix'")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).ok()?;
    let first = parsed.first()?;
    let prefix = first.get("value")?.as_str()?;

    if prefix.is_empty() {
        return None;
    }

    Some(prefix.to_string())
}
