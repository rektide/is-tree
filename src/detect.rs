use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoType {
    Git,
    Jujutsu,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub repo_type: RepoType,
    pub is_worktree: bool,
}

pub fn get_change_date(path: &Path) -> Option<String> {
    use std::fs;

    if let Ok(metadata) = fs::metadata(path) {
        if let Ok(modified) = metadata.modified() {
            use chrono::DateTime;
            let datetime: DateTime<chrono::Utc> = modified.into();
            return Some(datetime.format("%Y-%m-%d %H:%M:%S %z").to_string());
        }
    }
    None
}

pub fn get_commit_date(path: &Path, info: &RepoInfo) -> Option<String> {
    use std::process::Command;

    match info.repo_type {
        RepoType::Git => {
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
        RepoType::Jujutsu => {
            let repo_path = if info.is_worktree {
                let repo_file = path.join(".jj/repo");
                std::fs::read_to_string(repo_file).ok().map(|s| s.trim().to_string())
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
        RepoType::None => None,
    }
}

pub fn detect_repo(path: &Path) -> RepoInfo {
    let jj_path = path.join(".jj");
    let git_path = path.join(".git");

    if jj_path.exists() {
        let is_worktree = is_jujutsu_worktree(&jj_path);
        return RepoInfo {
            repo_type: RepoType::Jujutsu,
            is_worktree,
        };
    }

    if git_path.exists() {
        let is_worktree = is_git_worktree(&git_path);
        return RepoInfo {
            repo_type: RepoType::Git,
            is_worktree,
        };
    }

    RepoInfo {
        repo_type: RepoType::None,
        is_worktree: false,
    }
}

fn is_jujutsu_worktree(jj_path: &Path) -> bool {
    let repo_path = jj_path.join("repo");
    if repo_path.is_file() {
        return true;
    }
    false
}

fn is_git_worktree(git_path: &Path) -> bool {
    if git_path.is_file() {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none_directory() {
        let info = detect_repo(Path::new("/tmp/nonexistent"));
        assert_eq!(info.repo_type, RepoType::None);
        assert!(!info.is_worktree);
    }
}
