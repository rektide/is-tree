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
    repo_path.is_file()
}

fn is_git_worktree(git_path: &Path) -> bool {
    git_path.is_file()
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
