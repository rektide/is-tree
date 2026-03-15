use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;

mod detect;

use detect::{
    detect_repo, get_ahead, get_beads_prefix, get_change_date, get_commit_date, get_variant,
    get_workparent, RepoType,
};

#[derive(Parser)]
#[command(
    name = "is-tree",
    after_long_help = "
DETAILED OPTIONS:

  --filter <TYPES>
      Filter results by repository types. Multiple types can be comma-separated.
      Use - suffix for NOT (exclude matching types).
      
      Types: git, jj, worktree, worktree-git, worktree-jj
      
      Examples:
        --filter git              Show only Git repositories
        --filter git,jj           Show Git and Jujutsu repos
        --filter worktree-         Show non-worktree repos
        --filter git,jj,worktree- Show Git and JJ but exclude worktrees

  --sort <COLUMNS>
      Sort results by column(s). Multiple columns can be comma-separated.
      Use + suffix for ascending (default), - for descending.
      
      Columns: status, directory, commit-date, change-date, workparent, variant, ahead, beads
      
      Examples:
        --sort status+              Sort by status ascending
        --sort change-date-          Sort by most recent change first
        --sort status-,directory+    Sort by status descending, then directory

  --format <STRING>
      Custom output format using {column} placeholders.
      Use --format all as a shortcut for all columns.
      
      Columns: status, directory, commit-date, change-date, workparent, variant, ahead, beads
      
      Examples:
        --format '{status} {directory}'
        --format '{directory} - {status} ({workparent})'
        --format '{directory} ({variant})'
        --format all
"
)]
struct Cli {
    #[arg(short, long)]
    all: bool,

    #[arg(long)]
    filter: Option<String>,

    #[arg(long)]
    sort: Option<String>,

    #[arg(long)]
    format: Option<String>,

    #[arg(long)]
    date: Option<String>,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    header: bool,

    #[arg(long, default_value = " ")]
    separator: String,

    #[arg(long)]
    jj: bool,

    #[arg(long)]
    git: bool,

    #[arg(long)]
    beads: bool,

    #[arg(name = "DIRECTORIES")]
    directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
struct Result {
    status: String,
    directory: String,
    commit_date: Option<String>,
    change_date: Option<String>,
    workparent: Option<String>,
    variant: Option<String>,
    ahead: Option<isize>,
    beads: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    change_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workparent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    variant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ahead: Option<isize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    beads: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    run_list(cli);
}

fn run_list(args: Cli) {
    let filters = parse_filters(args.filter.as_deref());
    let sort_specs = parse_sort_specs(args.sort.as_deref());
    let format_str = build_format_string(&args);

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
        let commit_date = get_commit_date(&path, &info);
        let change_date = get_change_date(&path);
        let workparent = get_workparent(&path, &info);
        let variant = get_variant(&path, &info);
        let ahead = get_ahead(&path, &info);
        let beads = get_beads_prefix(&path);

        if matches_filters(&filters, &info, status) {
            results.push(Result {
                status: status.to_string(),
                directory: path.display().to_string(),
                commit_date,
                change_date,
                workparent,
                variant,
                ahead,
                beads,
            });
        }
    }

    sort_results(&mut results, &sort_specs);

    let format_str = format_str.as_deref().unwrap_or("{status} {directory}");

    if args.json {
        let columns = parse_columns_from_format(format_str);
        let json_results: Vec<JsonResult> = results
            .iter()
            .map(|r| filter_json_result(r, &columns))
            .collect();
        let json_output = serde_json::to_string_pretty(&json_results).unwrap();
        println!("{}", json_output);
    } else {
        let sep = &args.separator;
        if args.header {
            println!("{}", format_header(format_str, sep));
        }
        for result in results {
            let formatted = format_result(&result, format_str, sep);
            println!("{}", formatted);
        }
    }
}

fn build_format_string(args: &Cli) -> Option<String> {
    if let Some(fmt) = &args.format {
        return Some(resolve_format_shortcuts(fmt).to_string());
    }

    let mut columns = vec!["{status}", "{directory}"];

    if args.jj {
        columns.push("{ahead}");
    }

    if args.git {
        // Git-specific columns would go here when implemented
    }

    if args.beads {
        columns.push("{beads}");
    }

    Some(columns.join(" "))
}

fn resolve_format_shortcuts(format: &str) -> &str {
    if format == "all" {
        "{status} {directory} {commit-date} {change-date} {workparent} {variant} {ahead} {beads}"
    } else {
        format
    }
}

fn parse_columns_from_format(format: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut chars = format.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut col = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next();
                    break;
                }
                col.push(chars.next().unwrap());
            }
            let trimmed = col.trim();
            if !trimmed.is_empty() {
                columns.push(trimmed.to_string());
            }
        }
    }

    columns
}

fn filter_json_result(result: &Result, columns: &[String]) -> JsonResult {
    JsonResult {
        status: if columns.contains(&"status".to_string()) {
            Some(result.status.clone())
        } else {
            None
        },
        directory: if columns.contains(&"directory".to_string()) {
            Some(result.directory.clone())
        } else {
            None
        },
        commit_date: if columns.contains(&"commit-date".to_string()) {
            result.commit_date.clone()
        } else {
            None
        },
        change_date: if columns.contains(&"change-date".to_string()) {
            result.change_date.clone()
        } else {
            None
        },
        workparent: if columns.contains(&"workparent".to_string()) {
            result.workparent.clone()
        } else {
            None
        },
        variant: if columns.contains(&"variant".to_string()) {
            result.variant.clone()
        } else {
            None
        },
        ahead: if columns.contains(&"ahead".to_string()) {
            result.ahead
        } else {
            None
        },
        beads: if columns.contains(&"beads".to_string()) {
            result.beads.clone()
        } else {
            None
        },
    }
}

fn format_header(format_str: &str, separator: &str) -> String {
    let mut output = format_str.to_string();
    output = output.replace("{status}", "STATUS");
    output = output.replace("{directory}", "DIRECTORY");
    output = output.replace("{commit-date}", "COMMIT-DATE");
    output = output.replace("{change-date}", "CHANGE-DATE");
    output = output.replace("{workparent}", "WORKPARENT");
    output = output.replace("{variant}", "VARIANT");
    output = output.replace("{ahead}", "AHEAD");
    output = output.replace("{beads}", "BEADS");
    if separator != " " {
        output = output.replace(" ", separator);
    }
    output
}

fn format_result(result: &Result, format_str: &str, separator: &str) -> String {
    let mut output = format_str.to_string();
    output = output.replace("{status}", &result.status);
    output = output.replace("{directory}", &result.directory);
    output = output.replace("{commit-date}", result.commit_date.as_deref().unwrap_or(""));
    output = output.replace("{change-date}", result.change_date.as_deref().unwrap_or(""));
    output = output.replace("{workparent}", result.workparent.as_deref().unwrap_or(""));
    output = output.replace("{variant}", result.variant.as_deref().unwrap_or(""));
    output = output.replace(
        "{ahead}",
        &result.ahead.map(|x| x.to_string()).unwrap_or_default(),
    );
    output = output.replace("{beads}", result.beads.as_deref().unwrap_or(""));
    if separator != " " {
        output = output.replace(" ", separator);
    }
    output
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

            specs.push(SortSpec { column, descending });
        }
    }

    specs
}

fn sort_results(results: &mut [Result], sort_specs: &[SortSpec]) {
    if sort_specs.is_empty() {
        return;
    }

    results.sort_by(|a, b| {
        for spec in sort_specs {
            let ordering = match spec.column.as_str() {
                "status" => a.status.cmp(&b.status),
                "directory" => a.directory.cmp(&b.directory),
                "commit-date" => compare_option_dates(&a.commit_date, &b.commit_date),
                "change-date" => compare_option_dates(&a.change_date, &b.change_date),
                "workparent" => compare_options(&a.workparent, &b.workparent),
                "variant" => compare_options(&a.variant, &b.variant),
                "ahead" => compare_options(&a.ahead, &b.ahead),
                "beads" => compare_options(&a.beads, &b.beads),
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

fn compare_option_dates(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(da), Some(db)) => da.cmp(db),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_options<T: Ord>(a: &Option<T>, b: &Option<T>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(da), Some(db)) => da.cmp(db),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
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
