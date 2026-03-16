use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::detect::{
    detect_repo, get_ahead, get_beads_prefix, get_change_date, get_commit_date, get_variant,
    get_workparent, RepoInfo, RepoType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(pub &'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellValue {
    Text(String),
    Number(isize),
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    pub id: ColumnId,
    pub title: &'static str,
    pub description: &'static str,
    pub sortable: bool,
    pub default_in_base_format: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRow {
    pub path: PathBuf,
    pub cells: BTreeMap<ColumnId, CellValue>,
}

impl OutputRow {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cells: BTreeMap::new(),
        }
    }

    pub fn set_text(&mut self, id: ColumnId, value: impl Into<String>) {
        self.cells.insert(id, CellValue::Text(value.into()));
    }

    pub fn set_number(&mut self, id: ColumnId, value: isize) {
        self.cells.insert(id, CellValue::Number(value));
    }

    pub fn set_empty(&mut self, id: ColumnId) {
        self.cells.insert(id, CellValue::Empty);
    }
}

#[derive(Debug, Clone)]
pub struct DetectionCtx<'a> {
    pub path: &'a Path,
    pub repo: &'a RepoInfo,
}

#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub plugin_id: &'static str,
    pub arg: Arg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    pub enabled: bool,
    pub options: BTreeMap<String, String>,
}

impl PluginConfig {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            options: BTreeMap::new(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            options: BTreeMap::new(),
        }
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self::enabled()
    }
}

pub trait RepoProbe: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, path: &Path) -> RepoInfo;
}

pub trait DetectorPlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn columns(&self) -> &'static [ColumnSpec];

    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn configure(&self, _matches: &ArgMatches) -> PluginConfig {
        PluginConfig::enabled()
    }

    fn applies_to(&self, repo: &RepoInfo) -> bool;
    fn collect(&self, ctx: &DetectionCtx<'_>, cfg: &PluginConfig, row: &mut OutputRow);
}

pub struct CoreRepoProbe;

impl RepoProbe for CoreRepoProbe {
    fn id(&self) -> &'static str {
        "core-repo-probe"
    }

    fn detect(&self, path: &Path) -> RepoInfo {
        detect_repo(path)
    }
}

pub struct PluginRegistry {
    repo_probe: Box<dyn RepoProbe>,
    plugins: Vec<Box<dyn DetectorPlugin>>,
    by_id: BTreeMap<&'static str, usize>,
    column_owner: BTreeMap<ColumnId, &'static str>,
    all_columns: Vec<ColumnSpec>,
}

impl PluginRegistry {
    pub fn new(repo_probe: Box<dyn RepoProbe>) -> Self {
        Self {
            repo_probe,
            plugins: Vec::new(),
            by_id: BTreeMap::new(),
            column_owner: BTreeMap::new(),
            all_columns: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn DetectorPlugin>) {
        let plugin_id = plugin.id();
        if self.by_id.contains_key(plugin_id) {
            panic!("duplicate plugin id: {plugin_id}");
        }

        for column in plugin.columns() {
            if let Some(owner) = self.column_owner.get(&column.id) {
                panic!(
                    "duplicate column id '{}' in plugin '{}' (already owned by '{}')",
                    column.id.0, plugin_id, owner
                );
            }

            self.column_owner.insert(column.id, plugin_id);
            self.all_columns.push(*column);
        }

        self.by_id.insert(plugin_id, self.plugins.len());
        self.plugins.push(plugin);
    }

    pub fn build_command(&self, mut cmd: Command) -> Command {
        let mut arg_ids = HashSet::new();
        let mut arg_longs = HashSet::new();

        for plugin in &self.plugins {
            for spec in plugin.args() {
                if spec.plugin_id != plugin.id() {
                    panic!(
                        "plugin '{}' returned arg owned by '{}'",
                        plugin.id(),
                        spec.plugin_id
                    );
                }

                let arg_id = spec.arg.get_id().to_string();
                if !arg_ids.insert(arg_id.clone()) {
                    panic!(
                        "duplicate clap arg id '{arg_id}' from plugin '{}'",
                        plugin.id()
                    );
                }

                if let Some(long) = spec.arg.get_long() {
                    if !arg_longs.insert(long.to_string()) {
                        panic!(
                            "duplicate clap --long name '{long}' from plugin '{}'",
                            plugin.id()
                        );
                    }
                }

                cmd = cmd.arg(spec.arg);
            }
        }

        cmd
    }

    pub fn configure_all(&self, matches: &ArgMatches) -> BTreeMap<&'static str, PluginConfig> {
        let mut configs = BTreeMap::new();
        for plugin in &self.plugins {
            configs.insert(plugin.id(), plugin.configure(matches));
        }
        configs
    }

    pub fn columns(&self) -> &[ColumnSpec] {
        &self.all_columns
    }

    pub fn plugin_ids(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.id()).collect()
    }

    pub fn detect_repo(&self, path: &Path) -> RepoInfo {
        self.repo_probe.detect(path)
    }

    pub fn run_plugins(
        &self,
        ctx: &DetectionCtx<'_>,
        selected_plugins: &BTreeSet<&'static str>,
        configs: &BTreeMap<&'static str, PluginConfig>,
        row: &mut OutputRow,
    ) {
        for plugin in &self.plugins {
            let selected = selected_plugins.is_empty() || selected_plugins.contains(plugin.id());
            if !selected {
                continue;
            }

            let config = configs.get(plugin.id()).cloned().unwrap_or_default();
            if !config.enabled {
                continue;
            }

            if !plugin.applies_to(ctx.repo) {
                continue;
            }

            plugin.collect(ctx, &config, row);
        }
    }
}

pub fn default_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new(Box::new(CoreRepoProbe));

    registry.register(Box::new(CoreStatusPlugin));
    registry.register(Box::new(DatesPlugin));
    registry.register(Box::new(WorkparentPlugin));
    registry.register(Box::new(VariantPlugin));
    registry.register(Box::new(JjAheadPlugin));
    registry.register(Box::new(BeadsPrefixPlugin));

    registry
}

struct CoreStatusPlugin;

const CORE_STATUS_COLUMNS: &[ColumnSpec] = &[
    ColumnSpec {
        id: ColumnId("status"),
        title: "STATUS",
        description: "Repository status string",
        sortable: true,
        default_in_base_format: true,
    },
    ColumnSpec {
        id: ColumnId("directory"),
        title: "DIRECTORY",
        description: "Repository directory path",
        sortable: true,
        default_in_base_format: true,
    },
];

impl DetectorPlugin for CoreStatusPlugin {
    fn id(&self) -> &'static str {
        "core-status"
    }

    fn description(&self) -> &'static str {
        "Core status and directory columns"
    }

    fn columns(&self) -> &'static [ColumnSpec] {
        CORE_STATUS_COLUMNS
    }

    fn applies_to(&self, _repo: &RepoInfo) -> bool {
        true
    }

    fn collect(&self, ctx: &DetectionCtx<'_>, _cfg: &PluginConfig, row: &mut OutputRow) {
        row.set_text(ColumnId("status"), status_string(ctx.repo));
        row.set_text(ColumnId("directory"), ctx.path.display().to_string());
    }
}

struct DatesPlugin;

const DATES_COLUMNS: &[ColumnSpec] = &[
    ColumnSpec {
        id: ColumnId("commit-date"),
        title: "COMMIT-DATE",
        description: "Most recent commit date",
        sortable: true,
        default_in_base_format: false,
    },
    ColumnSpec {
        id: ColumnId("change-date"),
        title: "CHANGE-DATE",
        description: "Last filesystem change date",
        sortable: true,
        default_in_base_format: false,
    },
];

impl DetectorPlugin for DatesPlugin {
    fn id(&self) -> &'static str {
        "dates"
    }

    fn description(&self) -> &'static str {
        "Commit and change date columns"
    }

    fn columns(&self) -> &'static [ColumnSpec] {
        DATES_COLUMNS
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec {
                plugin_id: self.id(),
                arg: Arg::new("dates")
                    .long("dates")
                    .help("Enable both commit-date and change-date columns")
                    .action(ArgAction::SetTrue),
            },
            ArgSpec {
                plugin_id: self.id(),
                arg: Arg::new("commit-date")
                    .long("commit-date")
                    .help("Enable commit-date column")
                    .action(ArgAction::SetTrue),
            },
            ArgSpec {
                plugin_id: self.id(),
                arg: Arg::new("change-date")
                    .long("change-date")
                    .help("Enable change-date column")
                    .action(ArgAction::SetTrue),
            },
        ]
    }

    fn applies_to(&self, _repo: &RepoInfo) -> bool {
        true
    }

    fn collect(&self, ctx: &DetectionCtx<'_>, _cfg: &PluginConfig, row: &mut OutputRow) {
        if let Some(value) = get_commit_date(ctx.path, ctx.repo) {
            row.set_text(ColumnId("commit-date"), value);
        } else {
            row.set_empty(ColumnId("commit-date"));
        }

        if let Some(value) = get_change_date(ctx.path) {
            row.set_text(ColumnId("change-date"), value);
        } else {
            row.set_empty(ColumnId("change-date"));
        }
    }
}

struct WorkparentPlugin;

const WORKPARENT_COLUMNS: &[ColumnSpec] = &[ColumnSpec {
    id: ColumnId("workparent"),
    title: "WORKPARENT",
    description: "Parent workspace name for worktrees",
    sortable: true,
    default_in_base_format: false,
}];

impl DetectorPlugin for WorkparentPlugin {
    fn id(&self) -> &'static str {
        "workparent"
    }

    fn description(&self) -> &'static str {
        "Worktree parent name column"
    }

    fn columns(&self) -> &'static [ColumnSpec] {
        WORKPARENT_COLUMNS
    }

    fn applies_to(&self, repo: &RepoInfo) -> bool {
        repo.is_worktree
    }

    fn collect(&self, ctx: &DetectionCtx<'_>, _cfg: &PluginConfig, row: &mut OutputRow) {
        if let Some(value) = get_workparent(ctx.path, ctx.repo) {
            row.set_text(ColumnId("workparent"), value);
        } else {
            row.set_empty(ColumnId("workparent"));
        }
    }
}

struct VariantPlugin;

const VARIANT_COLUMNS: &[ColumnSpec] = &[ColumnSpec {
    id: ColumnId("variant"),
    title: "VARIANT",
    description: "Derived variant name for worktree directories",
    sortable: true,
    default_in_base_format: false,
}];

impl DetectorPlugin for VariantPlugin {
    fn id(&self) -> &'static str {
        "variant"
    }

    fn description(&self) -> &'static str {
        "Worktree variant column"
    }

    fn columns(&self) -> &'static [ColumnSpec] {
        VARIANT_COLUMNS
    }

    fn applies_to(&self, repo: &RepoInfo) -> bool {
        repo.is_worktree
    }

    fn collect(&self, ctx: &DetectionCtx<'_>, _cfg: &PluginConfig, row: &mut OutputRow) {
        if let Some(value) = get_variant(ctx.path, ctx.repo) {
            row.set_text(ColumnId("variant"), value);
        } else {
            row.set_empty(ColumnId("variant"));
        }
    }
}

struct JjAheadPlugin;

const JJ_AHEAD_COLUMNS: &[ColumnSpec] = &[ColumnSpec {
    id: ColumnId("ahead"),
    title: "AHEAD",
    description: "Local commits ahead of tracked remote bookmarks",
    sortable: true,
    default_in_base_format: false,
}];

impl DetectorPlugin for JjAheadPlugin {
    fn id(&self) -> &'static str {
        "jj-ahead"
    }

    fn description(&self) -> &'static str {
        "Jujutsu ahead count column"
    }

    fn columns(&self) -> &'static [ColumnSpec] {
        JJ_AHEAD_COLUMNS
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec {
            plugin_id: self.id(),
            arg: Arg::new("jj-ahead")
                .long("jj-ahead")
                .help("Enable jj ahead column")
                .action(ArgAction::SetTrue),
        }]
    }

    fn applies_to(&self, repo: &RepoInfo) -> bool {
        repo.repo_type == RepoType::Jujutsu
    }

    fn collect(&self, ctx: &DetectionCtx<'_>, _cfg: &PluginConfig, row: &mut OutputRow) {
        if let Some(value) = get_ahead(ctx.path, ctx.repo) {
            row.set_number(ColumnId("ahead"), value);
        } else {
            row.set_empty(ColumnId("ahead"));
        }
    }
}

struct BeadsPrefixPlugin;

const BEADS_COLUMNS: &[ColumnSpec] = &[ColumnSpec {
    id: ColumnId("beads"),
    title: "BEADS",
    description: "beads issue prefix from .beads config",
    sortable: true,
    default_in_base_format: false,
}];

impl DetectorPlugin for BeadsPrefixPlugin {
    fn id(&self) -> &'static str {
        "beads-prefix"
    }

    fn description(&self) -> &'static str {
        "beads prefix column"
    }

    fn columns(&self) -> &'static [ColumnSpec] {
        BEADS_COLUMNS
    }

    fn args(&self) -> Vec<ArgSpec> {
        vec![ArgSpec {
            plugin_id: self.id(),
            arg: Arg::new("beads")
                .long("beads")
                .help("Enable beads column")
                .action(ArgAction::SetTrue),
        }]
    }

    fn applies_to(&self, _repo: &RepoInfo) -> bool {
        true
    }

    fn collect(&self, ctx: &DetectionCtx<'_>, _cfg: &PluginConfig, row: &mut OutputRow) {
        if let Some(value) = get_beads_prefix(ctx.path) {
            row.set_text(ColumnId("beads"), value);
        } else {
            row.set_empty(ColumnId("beads"));
        }
    }
}

fn status_string(info: &RepoInfo) -> &'static str {
    match (&info.repo_type, info.is_worktree) {
        (RepoType::Git, true) => "worktree-git",
        (RepoType::Git, false) => "git",
        (RepoType::Jujutsu, true) => "worktree-jj",
        (RepoType::Jujutsu, false) => "jj",
        (RepoType::None, _) => "none",
    }
}
