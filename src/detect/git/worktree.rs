use std::path::Path;

pub fn is_worktree(git_path: &Path) -> bool {
    git_path.is_file()
}
