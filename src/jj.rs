use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::detect::{detect_repo, RepoType};

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub ahead: usize,
    pub bookmark: Option<String>,
    pub commits: Vec<CommitInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitInfo {
    pub id: String,
    pub message: String,
}

pub fn run_sync_status(directory: Option<std::path::PathBuf>) {
    let path = directory.unwrap_or_else(|| std::path::PathBuf::from("."));

    let info = detect_repo(&path);
    if info.repo_type != RepoType::Jujutsu {
        eprintln!("sync-status requires a jj repository");
        std::process::exit(1);
    }

    let status = get_sync_status(&path);

    if status.bookmark.is_none() {
        println!("No remote bookmark found");
        std::process::exit(0);
    }

    println!(
        "{} commits ahead of {}",
        status.ahead,
        status.bookmark.unwrap()
    );
    for commit in &status.commits {
        println!(
            "  {} {}",
            &commit.id[..12],
            commit.message.lines().next().unwrap_or("")
        );
    }
}

fn get_sync_status(path: &Path) -> SyncStatus {
    let output = Command::new("jj")
        .arg("-R")
        .arg(path)
        .arg("log")
        .arg("-r")
        .arg("ancestors(@) ~ ancestors(tracked_remote_bookmarks())")
        .arg("-T")
        .arg("commit_id.short() ++ \"\\n\" ++ description.first_line() ++ \"\\n---\\n\"")
        .arg("--no-pager")
        .arg("--no-graph")
        .output();

    let commits = match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            parse_commits(&stdout)
        }
        _ => Vec::new(),
    };

    let bookmark_output = Command::new("jj")
        .arg("-R")
        .arg(path)
        .arg("log")
        .arg("-r")
        .arg("tracked_remote_bookmarks()")
        .arg("-T")
        .arg("bookmarks")
        .arg("-n")
        .arg("1")
        .arg("--no-pager")
        .arg("--no-graph")
        .output();

    let bookmark = match bookmark_output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    };

    SyncStatus {
        ahead: commits.len(),
        bookmark,
        commits,
    }
}

fn parse_commits(output: &str) -> Vec<CommitInfo> {
    let mut commits = Vec::new();
    let mut current_id = None;
    let mut current_message = None;

    for line in output.lines() {
        if line == "---" {
            if let (Some(id), Some(message)) = (current_id.take(), current_message.take()) {
                commits.push(CommitInfo { id, message });
            }
        } else if current_id.is_none() {
            current_id = Some(line.to_string());
        } else if current_message.is_none() {
            current_message = Some(line.to_string());
        }
    }

    commits
}
