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

pub fn get_variant(path: &Path, info: &RepoInfo) -> Option<String> {
    let workspace_name = path.file_name()?.to_str()?;

    let project_name = if info.is_worktree {
        get_workparent(path, info)?
    } else {
        return None;
    };

    let variant = compute_variant(workspace_name, &project_name);
    Some(variant)
}

fn compute_variant(workspace_name: &str, project_name: &str) -> String {
    if let Some(suffix) = workspace_name.strip_prefix(project_name) {
        strip_separator(suffix).to_string()
    } else if let Some(suffix) = extract_embedded_suffix(workspace_name, project_name) {
        strip_separator(&suffix).to_string()
    } else {
        String::new()
    }
}

fn strip_separator(s: &str) -> &str {
    s.strip_prefix('-')
        .or_else(|| s.strip_prefix('_'))
        .unwrap_or(s)
}

fn extract_embedded_suffix(workspace_name: &str, project_name: &str) -> Option<String> {
    let dash_pattern = format!("-{}-", project_name);
    let underscore_pattern = format!("_{}_", project_name);

    if let Some(pos) = workspace_name.find(&dash_pattern) {
        let suffix_start = pos + dash_pattern.len();
        Some(workspace_name[suffix_start..].to_string())
    } else if let Some(pos) = workspace_name.find(&underscore_pattern) {
        let suffix_start = pos + underscore_pattern.len();
        Some(workspace_name[suffix_start..].to_string())
    } else {
        None
    }
}

pub fn get_workparent(path: &Path, info: &RepoInfo) -> Option<String> {
    use std::fs;

    if !info.is_worktree {
        return None;
    }

    let parent_path = match info.repo_type {
        RepoType::Git => {
            let git_file = path.join(".git");
            if let Ok(content) = fs::read_to_string(&git_file) {
                if content.starts_with("gitdir: ") {
                    let gitdir = content.trim_start_matches("gitdir: ").trim();
                    let gitdir_path = Path::new(gitdir);
                    gitdir_path
                        .parent()?
                        .parent()?
                        .parent()?
                        .file_name()?
                        .to_str()
                        .map(String::from)
                } else {
                    None
                }
            } else {
                None
            }
        }
        RepoType::Jujutsu => {
            let repo_file = path.join(".jj/repo");
            if let Ok(content) = fs::read_to_string(&repo_file) {
                let repo_path = Path::new(content.trim());
                repo_path
                    .parent()?
                    .parent()?
                    .file_name()?
                    .to_str()
                    .map(String::from)
            } else {
                None
            }
        }
        RepoType::None => None,
    };

    parent_path
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
