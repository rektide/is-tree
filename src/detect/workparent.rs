use std::fs;
use std::path::Path;

use super::repo::RepoInfo;

pub fn get_workparent(path: &Path, info: &RepoInfo) -> Option<String> {
    if !info.is_worktree {
        return None;
    }

    let parent_path = match info.repo_type {
        super::repo::RepoType::Git => {
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
        super::repo::RepoType::Jujutsu => {
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
        super::repo::RepoType::None => None,
    };

    parent_path
}
