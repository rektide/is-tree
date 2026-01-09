use std::env;
use std::path::PathBuf;

mod detect;

use detect::{detect_repo, RepoType};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: is-tree <directory> [directories...]");
        std::process::exit(1);
    }

    for arg in args {
        let path = PathBuf::from(&arg);
        let info = detect_repo(&path);
        let status = get_status_string(&info);
        println!("{} {}", status, path.display());
    }
}

fn get_status_string(info: &detect::RepoInfo) -> &'static str {
    match (&info.repo_type, info.is_worktree) {
        (RepoType::Git, true) => "worktree-git",
        (RepoType::Git, false) => "git",
        (RepoType::Jujutsu, true) => "worktree-jj",
        (RepoType::Jujutsu, false) => "jj",
        (RepoType::None, _) => "none",
    }
}
