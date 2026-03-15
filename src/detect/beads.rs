use std::path::Path;
use std::process::Command;

pub fn get_beads_prefix(path: &Path) -> Option<String> {
    let beads_dir = path.join(".beads");
    if !beads_dir.exists() {
        return None;
    }

    let db_path = beads_dir.join("beads.db");
    if !db_path.exists() {
        return None;
    }

    let output = Command::new("sqlite3")
        .arg(&db_path)
        .arg("SELECT value FROM config WHERE key = 'issue_prefix'")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let prefix = stdout.trim();
    if prefix.is_empty() {
        return None;
    }

    Some(prefix.to_string())
}
