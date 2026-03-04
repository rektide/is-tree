use std::path::Path;

pub fn is_worktree(jj_path: &Path) -> bool {
    jj_path.join("repo").is_file()
}
