use std::path::Path;

use super::repo::RepoInfo;
use super::workparent::get_workparent;

pub fn get_variant(path: &Path, info: &RepoInfo) -> Option<String> {
    let workspace_name = path.file_name()?.to_str()?;

    let project_name = if info.is_worktree {
        get_workparent(path, info)?
    } else {
        return None;
    };

    let variant = compute_variant(workspace_name, &project_name);
    Some(variant)
}

fn compute_variant(workspace_name: &str, project_name: &str) -> String {
    if let Some(suffix) = workspace_name.strip_prefix(project_name) {
        strip_separator(suffix).to_string()
    } else if let Some(suffix) = extract_embedded_suffix(workspace_name, project_name) {
        strip_separator(&suffix).to_string()
    } else {
        String::new()
    }
}

fn strip_separator(s: &str) -> &str {
    s.strip_prefix('-')
        .or_else(|| s.strip_prefix('_'))
        .unwrap_or(s)
}

fn extract_embedded_suffix(workspace_name: &str, project_name: &str) -> Option<String> {
    let dash_pattern = format!("-{}-", project_name);
    let underscore_pattern = format!("_{}_", project_name);

    if let Some(pos) = workspace_name.find(&dash_pattern) {
        let suffix_start = pos + dash_pattern.len();
        Some(workspace_name[suffix_start..].to_string())
    } else if let Some(pos) = workspace_name.find(&underscore_pattern) {
        let suffix_start = pos + underscore_pattern.len();
        Some(workspace_name[suffix_start..].to_string())
    } else {
        None
    }
}
