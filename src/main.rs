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

    #[arg(long)]
    sort: Option<String>,

    #[arg(name = "DIRECTORIES")]
    directories: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct Result {
    status: String,
    directory: PathBuf,
}

fn main() {
    let args = Args::parse();

    let filters = parse_filters(args.filter.as_deref());
    let sort_specs = parse_sort_specs(args.sort.as_deref());

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

    let mut results: Vec<Result> = Vec::new();

    for path in paths {
        let info = detect_repo(&path);
        let status = get_status_string(&info);

        if matches_filters(&filters, &info, &status) {
            results.push(Result {
                status: status.to_string(),
                directory: path,
            });
        }
    }

    sort_results(&mut results, &sort_specs);

    for result in results {
        println!("{} {}", result.status, result.directory.display());
    }
}

#[derive(Debug, Clone)]
struct Filter {
    value: String,
    negate: bool,
}

#[derive(Debug, Clone)]
struct SortSpec {
    column: String,
    descending: bool,
}

fn parse_sort_specs(sort_str: Option<&str>) -> Vec<SortSpec> {
    let mut specs = Vec::new();

    if let Some(s) = sort_str {
        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            let descending = part.ends_with('-');
            let ascending = part.ends_with('+');

            let column = if descending || ascending {
                part[..part.len() - 1].to_string()
            } else {
                part.to_string()
            };

            let descending = if descending {
                true
            } else if ascending {
                false
            } else {
                false
            };

            specs.push(SortSpec { column, descending });
        }
    }

    specs
}

fn sort_results(results: &mut Vec<Result>, sort_specs: &[SortSpec]) {
    if sort_specs.is_empty() {
        return;
    }

    results.sort_by(|a, b| {
        for spec in sort_specs {
            let ordering = match spec.column.as_str() {
                "status" => a.status.cmp(&b.status),
                "directory" => compare_paths(&a.directory, &b.directory),
                _ => std::cmp::Ordering::Equal,
            };

            if ordering != std::cmp::Ordering::Equal {
                return if spec.descending {
                    ordering.reverse()
                } else {
                    ordering
                };
            }
        }
        std::cmp::Ordering::Equal
    });
}

fn compare_paths(a: &Path, b: &Path) -> std::cmp::Ordering {
    a.display().to_string().cmp(&b.display().to_string())
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
