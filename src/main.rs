use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{value_parser, Arg, ArgMatches, Command};
use serde_json::Value;

mod detect;
mod plugin;

use plugin::{default_registry, CellValue, ColumnCatalog, OutputRow, PluginResult};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> PluginResult<()> {
    let registry = default_registry();
    let command = registry.build_command(base_command());
    let matches = command.get_matches();

    let paths = resolve_paths(&matches)?;
    let explicit_format = matches.get_one::<String>("format").map(String::as_str);

    if should_short_circuit_directory_only(explicit_format) {
        return render_directory_only(
            &paths,
            matches.get_flag("json"),
            matches.get_flag("header"),
        );
    }

    let mut requested_mask =
        resolve_requested_mask(explicit_format, registry.columns(), &registry)?;
    registry.augment_requested_column_mask_from_args(&matches, &mut requested_mask)?;

    let format_template = resolve_format_template(explicit_format, registry.columns(), &requested_mask)?;

    let selected_plugin_ids = parse_csv(matches.get_one::<String>("plugins"));
    let selected_plugin_indexes = registry.resolve_plugin_indexes(&selected_plugin_ids)?;
    let configs = registry.configure_all(&matches);
    let items = registry.probe_items(&paths);
    let mut rows = registry.init_rows(&items);
    let microbatch_rows = matches
        .get_one::<usize>("microbatch-rows")
        .copied()
        .unwrap_or(64);

    registry
        .run_plugins_streaming(
            &items,
            &selected_plugin_indexes,
            &configs,
            &requested_mask,
            &mut rows,
            microbatch_rows,
        )
        .await?;

    if matches.get_flag("json") {
        render_json(&rows, &format_template, registry.columns())?;
    } else {
        let separator = matches
            .get_one::<String>("separator")
            .map(String::as_str)
            .unwrap_or(" ");

        if matches.get_flag("header") {
            println!("{}", format_header(&format_template, separator, registry.columns()));
        }

        for row in &rows {
            println!("{}", format_row(row, &format_template, separator, registry.columns()));
        }
    }

    Ok(())
}

fn base_command() -> Command {
    Command::new("is-tree")
        .about("Indexed plugin-based repository detector")
        .arg(
            Arg::new("all")
                .short('a')
                .long("all")
                .help("Scan all non-hidden subdirectories in current directory")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .help("Output format template with {column} placeholders; use 'all' for all columns"),
        )
        .arg(
            Arg::new("plugins")
                .long("plugins")
                .value_name("PLUGIN_IDS")
                .help("Comma-separated plugin ids to run (default: all registered plugins)"),
        )
        .arg(
            Arg::new("microbatch-rows")
                .long("microbatch-rows")
                .value_name("COUNT")
                .help("Maximum row patches per emitted microbatch")
                .value_parser(value_parser!(usize))
                .default_value("64"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Render output as JSON")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("header")
                .long("header")
                .help("Render a header row for text output")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("separator")
                .long("separator")
                .value_name("STRING")
                .help("Replace spaces in rendered text output")
                .default_value(" "),
        )
        .arg(
            Arg::new("directories")
                .value_name("DIRECTORIES")
                .help("Directories to inspect")
                .num_args(1..),
        )
}

fn resolve_paths(matches: &ArgMatches) -> PluginResult<Vec<PathBuf>> {
    if matches.get_flag("all") {
        let current_dir = Path::new(".");
        return Ok(get_subdirectories(current_dir)
            .into_iter()
            .map(|name| current_dir.join(name))
            .collect());
    }

    let directories: Vec<PathBuf> = matches
        .get_many::<String>("directories")
        .map(|values| values.map(PathBuf::from).collect())
        .unwrap_or_default();

    if directories.is_empty() {
        return Err("Usage: is-tree <directory> [directories...] | --all".to_string());
    }

    Ok(directories)
}

fn resolve_requested_mask(
    explicit_format: Option<&str>,
    columns: &ColumnCatalog,
    registry: &plugin::PluginRegistry,
) -> PluginResult<Vec<bool>> {
    if let Some(format) = explicit_format {
        if format == "all" {
            return Ok(vec![true; columns.len()]);
        }

        let keys = parse_columns_from_format(format);
        return registry.resolve_requested_column_mask(&keys);
    }

    let mut mask = vec![false; columns.len()];
    for column in &columns.columns {
        if column.default_in_base_format {
            mask[column.ix] = true;
        }
    }

    Ok(mask)
}

fn resolve_format_template(
    explicit_format: Option<&str>,
    columns: &ColumnCatalog,
    requested_mask: &[bool],
) -> PluginResult<String> {
    if let Some(format) = explicit_format {
        if format == "all" {
            return Ok(all_columns_template(columns));
        }

        return Ok(format.to_string());
    }

    let keys: Vec<String> = columns
        .columns
        .iter()
        .filter(|column| requested_mask.get(column.ix).copied().unwrap_or(false))
        .map(|column| column.key.to_string())
        .collect();

    if keys.is_empty() {
        return Err(
            "No columns selected. Use --format, --jj, --jj-ahead, or plugin-specific toggles."
                .to_string(),
        );
    }

    Ok(keys
        .into_iter()
        .map(|key| format!("{{{key}}}"))
        .collect::<Vec<_>>()
        .join(" "))
}

fn all_columns_template(columns: &ColumnCatalog) -> String {
    columns
        .columns
        .iter()
        .map(|column| format!("{{{}}}", column.key))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_columns_from_format(format: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut chars = format.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '{' {
            continue;
        }

        let mut key = String::new();
        while let Some(&next) = chars.peek() {
            if next == '}' {
                chars.next();
                break;
            }
            key.push(chars.next().unwrap_or_default());
        }

        let trimmed = key.trim();
        if !trimmed.is_empty() {
            columns.push(trimmed.to_string());
        }
    }

    columns
}

fn parse_csv(value: Option<&String>) -> Vec<String> {
    match value {
        Some(v) => v
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        None => Vec::new(),
    }
}

fn should_short_circuit_directory_only(explicit_format: Option<&str>) -> bool {
    matches!(
        explicit_format.map(str::trim),
        Some("directory") | Some("{directory}")
    )
}

fn render_directory_only(paths: &[PathBuf], as_json: bool, header: bool) -> PluginResult<()> {
    if as_json {
        let items: Vec<Value> = paths
            .iter()
            .map(|path| {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "directory".to_string(),
                    Value::String(path.display().to_string()),
                );
                Value::Object(obj)
            })
            .collect();

        let output = serde_json::to_string_pretty(&items).map_err(|err| err.to_string())?;
        println!("{output}");
        return Ok(());
    }

    if header {
        println!("directory");
    }

    for path in paths {
        println!("{}", path.display());
    }

    Ok(())
}

fn format_header(template: &str, separator: &str, columns: &ColumnCatalog) -> String {
    let mut output = template.to_string();
    for column in &columns.columns {
        let placeholder = format!("{{{}}}", column.key);
        output = output.replace(&placeholder, column.title);
    }

    if separator != " " {
        output = output.replace(' ', separator);
    }

    output
}

fn format_row(row: &OutputRow, template: &str, separator: &str, columns: &ColumnCatalog) -> String {
    let mut output = template.to_string();
    for column in &columns.columns {
        let placeholder = format!("{{{}}}", column.key);
        let value = row
            .cells
            .get(column.ix)
            .map(cell_to_text)
            .unwrap_or_default();
        output = output.replace(&placeholder, &value);
    }

    if separator != " " {
        output = output.replace(' ', separator);
    }

    output
}

fn cell_to_text(cell: &CellValue) -> String {
    match cell {
        CellValue::Text(value) => value.clone(),
        CellValue::Number(value) => value.to_string(),
        CellValue::Empty => String::new(),
    }
}

fn render_json(rows: &[OutputRow], template: &str, columns: &ColumnCatalog) -> PluginResult<()> {
    let keys = unique_preserving_order(parse_columns_from_format(template));
    let mut items = Vec::with_capacity(rows.len());

    for row in rows {
        let mut obj = serde_json::Map::new();

        for key in &keys {
            let Some(ix) = columns.column_ix(key) else {
                continue;
            };
            let Some(cell) = row.cells.get(ix) else {
                continue;
            };

            if let Some(value) = cell_to_json(cell) {
                obj.insert(key.clone(), value);
            }
        }

        items.push(Value::Object(obj));
    }

    let output = serde_json::to_string_pretty(&items).map_err(|err| err.to_string())?;
    println!("{output}");
    Ok(())
}

fn cell_to_json(cell: &CellValue) -> Option<Value> {
    match cell {
        CellValue::Text(value) => Some(Value::String(value.clone())),
        CellValue::Number(value) => Some(Value::Number((*value).into())),
        CellValue::Empty => None,
    }
}

fn unique_preserving_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::should_short_circuit_directory_only;

    #[test]
    fn short_circuit_matches_directory_shorthand() {
        assert!(should_short_circuit_directory_only(Some("directory")));
    }

    #[test]
    fn short_circuit_matches_directory_placeholder() {
        assert!(should_short_circuit_directory_only(Some("{directory}")));
    }

    #[test]
    fn short_circuit_ignores_non_directory_formats() {
        assert!(!should_short_circuit_directory_only(Some("all")));
        assert!(!should_short_circuit_directory_only(Some("{status} {directory}")));
        assert!(!should_short_circuit_directory_only(None));
    }
}
