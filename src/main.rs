use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;

mod detect;

use detect::{detect_repo, RepoType};

#[derive(Parser)]
#[command(name = "is-tree")]
struct Args {
    #[arg(short, long)]
    all: bool,

    #[arg(long)]
    filter: Option<String>,

    #[arg(name = "DIRECTORIES")]
    directories: Vec<PathBuf>,
}

fn main() {
    let args = Args::parse();

    let filters = parse_filters(args.filter.as_deref());

    let paths = if args.all {
        let current_dir = Path::new(".");
        get_subdirectories(current_dir)
            .into_iter()
            .map(|p| current_dir.join(p))
            .collect()
    } else if args.directories.is_empty() {
        eprintln!("Usage: is-tree <directory> [directories...] | --all");
        std::process::exit(1);
    } else {
        args.directories
    };

    for path in paths {
        let info = detect_repo(&path);
        let status = get_status_string(&info);

        if matches_filters(&filters, &info, &status) {
            println!("{} {}", status, path.display());
        }
    }
}

#[derive(Debug, Clone)]
struct Filter {
    value: String,
    negate: bool,
}

fn parse_filters(filter_str: Option<&str>) -> Vec<Filter> {
    let mut filters = Vec::new();

    if let Some(s) = filter_str {
        for part in s.split(',') {
            let negate = part.ends_with('-');
            let value = if negate {
                part.trim_end_matches('-').to_string()
            } else {
                part.to_string()
            };
            filters.push(Filter { value, negate });
        }
    }

    filters
}

fn matches_filters(filters: &[Filter], info: &detect::RepoInfo, status: &str) -> bool {
    if filters.is_empty() {
        return true;
    }

    let has_positive_filters = filters.iter().any(|f| !f.negate);
    let mut included = false;
    let mut excluded = false;

    for filter in filters {
        let matches = match filter.value.as_str() {
            "git" => status == "git",
            "jj" => status == "jj",
            "worktree" => info.is_worktree,
            "worktree-git" => status == "worktree-git",
            "worktree-jj" => status == "worktree-jj",
            _ => false,
        };

        if !filter.negate && matches {
            included = true;
        }
        if filter.negate && matches {
            excluded = true;
        }
    }

    if has_positive_filters {
        included && !excluded
    } else {
        !excluded
    }
}

fn get_subdirectories(dir: &Path) -> Vec<String> {
    let mut dirs = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') {
                            dirs.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    dirs.sort();
    dirs
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
