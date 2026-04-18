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
    if let Some(suffix) = try_strip_project(workspace_name, project_name) {
        return strip_separator(&suffix).to_string();
    }
    String::new()
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn try_strip_project(workspace_name: &str, project_name: &str) -> Option<String> {
    let norm_workspace = normalize(workspace_name);
    let norm_project = normalize(project_name);

    let prefix_len = norm_project.len();
    if norm_workspace.len() <= prefix_len {
        return None;
    }
    if !norm_workspace.starts_with(&norm_project) {
        return None;
    }

    let norm_suffix = &norm_workspace[prefix_len..];
    let suffix_len = norm_suffix.len();

    Some(workspace_name[workspace_name.len() - suffix_len..].to_string())
}

fn strip_separator(s: &str) -> &str {
    s.strip_prefix('-')
        .or_else(|| s.strip_prefix('_'))
        .or_else(|| s.strip_prefix('.'))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_dash_suffix() {
        assert_eq!(compute_variant("myproject-foo", "myproject"), "foo");
    }

    #[test]
    fn dot_in_project_name() {
        assert_eq!(compute_variant("usegpu-viteplus", "use.gpu"), "viteplus");
    }

    #[test]
    fn dot_in_project_name_dom() {
        assert_eq!(compute_variant("usegpu-dom", "use.gpu"), "dom");
    }

    #[test]
    fn dot_in_project_name_rolldown() {
        assert_eq!(compute_variant("usegpu-rolldown", "use.gpu"), "rolldown");
    }

    #[test]
    fn no_match() {
        assert_eq!(compute_variant("unrelated-name", "other-project"), "");
    }

    #[test]
    fn exact_match_no_variant() {
        assert_eq!(compute_variant("myproject", "myproject"), "");
    }

    #[test]
    fn underscore_suffix() {
        assert_eq!(compute_variant("myproject_foo", "myproject"), "foo");
    }

    #[test]
    fn dot_suffix() {
        assert_eq!(compute_variant("myproject.foo", "myproject"), "foo");
    }
}
