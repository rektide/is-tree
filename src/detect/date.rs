use std::path::Path;
use std::process::Command;

use super::repo::RepoInfo;

pub fn get_change_date(path: &Path) -> Option<String> {
    if let Ok(metadata) = std::fs::metadata(path) {
        if let Ok(modified) = metadata.modified() {
            use chrono::DateTime;
            let datetime: DateTime<chrono::Utc> = modified.into();
            return Some(datetime.format("%Y-%m-%d %H:%M:%S %z").to_string());
        }
    }
    None
}

pub fn get_commit_date(path: &Path, info: &RepoInfo) -> Option<String> {
    match info.repo_type {
        super::repo::RepoType::Git => {
            let output = Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("log")
                .arg("-1")
                .arg("--format=%ci")
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let trimmed = stdout.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                _ => {}
            }
            None
        }
        super::repo::RepoType::Jujutsu => {
            let repo_path = if info.is_worktree {
                let repo_file = path.join(".jj/repo");
                std::fs::read_to_string(repo_file)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                Some(path.join(".jj/repo").display().to_string())
            };

            if let Some(repo) = repo_path {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .arg("log")
                    .arg("-1")
                    .arg("--format=%ci")
                    .output();

                match output {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        let trimmed = stdout.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        super::repo::RepoType::None => None,
    }
}
