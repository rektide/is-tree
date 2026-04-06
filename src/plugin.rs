use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;

use clap::{Arg, ArgAction, ArgMatches, Command};
use futures_core::Stream;
use futures_util::stream;
use futures_util::StreamExt;

use crate::detect::{
    detect_repo, get_ahead, get_beads_last_changed, RepoInfo, RepoType,
};

pub type RowId = usize;
pub type PluginIx = usize;
pub type ColumnIx = usize;
pub type ArgId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoWorkItem {
    pub row_id: RowId,
    pub path: PathBuf,
    pub repo: RepoInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellValue {
    Text(String),
    Number(isize),
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnDecl {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub sortable: bool,
    pub default_in_base_format: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    pub ix: ColumnIx,
    pub key: &'static str,
    pub owner_plugin_ix: PluginIx,
    pub title: &'static str,
    pub description: &'static str,
    pub sortable: bool,
    pub default_in_base_format: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnCatalog {
    pub columns: Vec<ColumnSpec>,
    pub by_key: HashMap<&'static str, ColumnIx>,
    pub by_plugin: Vec<Vec<ColumnIx>>,
}

impl ColumnCatalog {
    fn new() -> Self {
        Self {
            columns: Vec::new(),
            by_key: HashMap::new(),
            by_plugin: Vec::new(),
        }
    }

    fn register_plugin_slot(&mut self) {
        self.by_plugin.push(Vec::new());
    }

    fn push_column(&mut self, plugin_ix: PluginIx, decl: ColumnDecl) {
        if self.by_key.contains_key(decl.key) {
            panic!("duplicate column key '{}'", decl.key);
        }

        let ix = self.columns.len();
        self.columns.push(ColumnSpec {
            ix,
            key: decl.key,
            owner_plugin_ix: plugin_ix,
            title: decl.title,
            description: decl.description,
            sortable: decl.sortable,
            default_in_base_format: decl.default_in_base_format,
        });
        self.by_key.insert(decl.key, ix);
        self.by_plugin
            .get_mut(plugin_ix)
            .expect("plugin slot exists")
            .push(ix);
    }

    pub fn column_ix(&self, key: &str) -> Option<ColumnIx> {
        self.by_key.get(key).copied()
    }

    pub fn plugin_columns(&self, plugin_ix: PluginIx) -> &[ColumnIx] {
        self.by_plugin
            .get(plugin_ix)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRow {
    pub row_id: RowId,
    pub path: PathBuf,
    pub cells: Vec<CellValue>,
}

impl OutputRow {
    pub fn new(row_id: RowId, path: PathBuf, width: usize) -> Self {
        Self {
            row_id,
            path,
            cells: vec![CellValue::Empty; width],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowPatch {
    pub row_id: RowId,
    pub updates: Vec<(ColumnIx, CellValue)>,
}

pub type MicroBatch = Vec<RowPatch>;
pub type PluginError = String;
pub type PluginResult<T> = Result<T, PluginError>;
pub type BatchStream<'a> = Pin<Box<dyn Stream<Item = PluginResult<MicroBatch>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    PluginToggle {
        help: &'static str,
    },
    ColumnToggle {
        local_col_ix: usize,
        help: &'static str,
    },
    Flag {
        suffix: &'static str,
        help: &'static str,
        default: bool,
    },
    String {
        suffix: &'static str,
        help: &'static str,
        value_name: &'static str,
        default: Option<&'static str>,
    },
    StringList {
        suffix: &'static str,
        help: &'static str,
        value_name: &'static str,
    },
    Count {
        suffix: &'static str,
        help: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgValue {
    Bool(bool),
    String(Option<String>),
    Strings(Vec<String>),
    Count(u8),
}

#[derive(Debug, Clone)]
struct ArgEntry {
    plugin_ix: PluginIx,
    local_arg_ix: usize,
    kind: ArgKind,
    clap_id: String,
    long: String,
}

#[derive(Debug, Clone)]
pub struct ArgCatalog {
    entries: Vec<ArgEntry>,
    by_plugin: Vec<Vec<ArgId>>,
    by_long: HashMap<String, ArgId>,
}

impl ArgCatalog {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            by_plugin: Vec::new(),
            by_long: HashMap::new(),
        }
    }

    fn register_plugin_slot(&mut self) {
        self.by_plugin.push(Vec::new());
    }

    fn push_arg(
        &mut self,
        plugin_ix: PluginIx,
        local_arg_ix: usize,
        kind: ArgKind,
        long: String,
    ) {
        if self.by_long.contains_key(&long) {
            panic!("duplicate arg long name '--{}'", long);
        }

        let arg_id = self.entries.len();
        let clap_id = format!("p{plugin_ix}a{local_arg_ix}");
        self.entries.push(ArgEntry {
            plugin_ix,
            local_arg_ix,
            kind,
            clap_id,
            long: long.clone(),
        });
        self.by_plugin
            .get_mut(plugin_ix)
            .expect("plugin slot exists")
            .push(arg_id);
        self.by_long.insert(long, arg_id);
    }

    fn plugin_args(&self, plugin_ix: PluginIx) -> &[ArgId] {
        self.by_plugin
            .get(plugin_ix)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn parse_store(&self, matches: &ArgMatches) -> ArgStore {
        let mut values = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let value = parse_arg_value(matches, entry);
            values.push(value);
        }

        ArgStore { values }
    }
}

#[derive(Debug, Clone)]
pub struct ArgStore {
    values: Vec<ArgValue>,
}

impl ArgStore {
    fn value(&self, arg_id: ArgId) -> Option<&ArgValue> {
        self.values.get(arg_id)
    }

    fn bool(&self, arg_id: ArgId) -> Option<bool> {
        match self.values.get(arg_id) {
            Some(ArgValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PluginArgs<'a> {
    arg_ids: &'a [ArgId],
    store: &'a ArgStore,
}

impl<'a> PluginArgs<'a> {
    pub fn bool(&self, local_arg_ix: usize) -> Option<bool> {
        let arg_id = *self.arg_ids.get(local_arg_ix)?;
        let value = self.store.value(arg_id)?;
        match value {
            ArgValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn string(&self, local_arg_ix: usize) -> Option<&str> {
        let arg_id = *self.arg_ids.get(local_arg_ix)?;
        let value = self.store.value(arg_id)?;
        match value {
            ArgValue::String(v) => v.as_deref(),
            _ => None,
        }
    }

    pub fn strings(&self, local_arg_ix: usize) -> Option<&[String]> {
        let arg_id = *self.arg_ids.get(local_arg_ix)?;
        let value = self.store.value(arg_id)?;
        match value {
            ArgValue::Strings(v) => Some(v),
            _ => None,
        }
    }

    pub fn count(&self, local_arg_ix: usize) -> Option<u8> {
        let arg_id = *self.arg_ids.get(local_arg_ix)?;
        let value = self.store.value(arg_id)?;
        match value {
            ArgValue::Count(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    pub enabled: bool,
    pub options: HashMap<String, String>,
}

impl PluginConfig {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            options: HashMap::new(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            options: HashMap::new(),
        }
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self::enabled()
    }
}

#[derive(Debug, Clone)]
pub struct CollectRequest<'a> {
    pub items: &'a [RepoWorkItem],
    pub cfg: &'a PluginConfig,
    pub requested_columns: &'a [(usize, ColumnIx)],
    pub microbatch_rows: usize,
}

pub trait RepoProbe: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, path: &Path) -> RepoInfo;
}

pub trait DetectorPlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn column_decls(&self) -> &'static [ColumnDecl];

    fn arg_kinds(&self) -> &'static [ArgKind] {
        &[]
    }

    fn configure(&self, _args: PluginArgs<'_>) -> PluginConfig {
        PluginConfig::enabled()
    }

    fn applies_to(&self, _repo: &RepoInfo) -> bool {
        true
    }

    fn collect_stream<'a>(&'a self, req: CollectRequest<'a>) -> BatchStream<'a>;
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
    plugin_by_id: HashMap<&'static str, PluginIx>,
    columns: ColumnCatalog,
    args: ArgCatalog,
}

impl PluginRegistry {
    pub fn new(repo_probe: Box<dyn RepoProbe>) -> Self {
        Self {
            repo_probe,
            plugins: Vec::new(),
            plugin_by_id: HashMap::new(),
            columns: ColumnCatalog::new(),
            args: ArgCatalog::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn DetectorPlugin>) {
        let plugin_id = plugin.id();
        if self.plugin_by_id.contains_key(plugin_id) {
            panic!("duplicate plugin id: {plugin_id}");
        }

        let plugin_ix = self.plugins.len();
        self.columns.register_plugin_slot();
        self.args.register_plugin_slot();

        let decls = plugin.column_decls();
        for decl in decls {
            self.columns.push_column(plugin_ix, *decl);
        }

        for (local_arg_ix, kind) in plugin.arg_kinds().iter().copied().enumerate() {
            let long = make_arg_long(plugin_id, kind, decls);
            self.args.push_arg(plugin_ix, local_arg_ix, kind, long);
        }

        self.plugin_by_id.insert(plugin_id, plugin_ix);
        self.plugins.push(plugin);
    }

    pub fn build_command(&self, mut cmd: Command) -> Command {
        let mut arg_ids = HashSet::new();
        for entry in &self.args.entries {
            if !arg_ids.insert(entry.clap_id.clone()) {
                panic!("duplicate clap arg id '{}'", entry.clap_id);
            }

            cmd = cmd.arg(build_clap_arg(entry));
        }

        cmd
    }

    pub fn configure_all(&self, matches: &ArgMatches) -> Vec<PluginConfig> {
        let store = self.args.parse_store(matches);
        let mut configs = Vec::with_capacity(self.plugins.len());
        for (plugin_ix, plugin) in self.plugins.iter().enumerate() {
            let args = PluginArgs {
                arg_ids: self.args.plugin_args(plugin_ix),
                store: &store,
            };
            configs.push(plugin.configure(args));
        }
        configs
    }

    pub fn columns(&self) -> &ColumnCatalog {
        &self.columns
    }

    pub fn plugin_ix(&self, plugin_id: &str) -> Option<PluginIx> {
        self.plugin_by_id.get(plugin_id).copied()
    }

    pub fn plugin_ids(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.id()).collect()
    }

    pub fn probe_items(&self, paths: &[PathBuf]) -> Vec<RepoWorkItem> {
        paths
            .iter()
            .enumerate()
            .map(|(row_id, path)| RepoWorkItem {
                row_id,
                path: path.clone(),
                repo: self.repo_probe.detect(path),
            })
            .collect()
    }

    pub fn init_rows(&self, items: &[RepoWorkItem]) -> Vec<OutputRow> {
        items
            .iter()
            .map(|item| OutputRow::new(item.row_id, item.path.clone(), self.columns.len()))
            .collect()
    }

    pub fn resolve_requested_column_mask(&self, keys: &[String]) -> PluginResult<Vec<bool>> {
        let mut mask = vec![false; self.columns.len()];
        for key in keys {
            let ix = self
                .columns
                .column_ix(key)
                .ok_or_else(|| format!("unknown column key: {key}"))?;
            mask[ix] = true;
        }
        Ok(mask)
    }

    pub fn resolve_plugin_indexes(&self, ids: &[String]) -> PluginResult<Vec<PluginIx>> {
        if ids.is_empty() {
            return Ok((0..self.plugins.len()).collect());
        }

        let mut resolved = Vec::with_capacity(ids.len());
        for id in ids {
            let ix = self
                .plugin_ix(id)
                .ok_or_else(|| format!("unknown plugin id: {id}"))?;
            resolved.push(ix);
        }
        Ok(resolved)
    }

    pub fn augment_requested_column_mask_from_args(
        &self,
        matches: &ArgMatches,
        requested_column_mask: &mut [bool],
    ) -> PluginResult<()> {
        let store = self.args.parse_store(matches);

        for plugin_ix in 0..self.plugins.len() {
            let mut enable_all_columns = false;

            for &arg_id in self.args.plugin_args(plugin_ix) {
                let entry = self
                    .args
                    .entries
                    .get(arg_id)
                    .ok_or_else(|| format!("unknown arg id: {arg_id}"))?;
                let enabled = store.bool(arg_id).unwrap_or(false);

                match entry.kind {
                    ArgKind::PluginToggle { .. } => {
                        if enabled {
                            enable_all_columns = true;
                        }
                    }
                    ArgKind::ColumnToggle { local_col_ix, .. } => {
                        if !enabled {
                            continue;
                        }

                        let global_ix = self
                            .columns
                            .plugin_columns(plugin_ix)
                            .get(local_col_ix)
                            .copied()
                            .ok_or_else(|| {
                                format!(
                                    "plugin '{}' references invalid local column index {}",
                                    self.plugins[plugin_ix].id(),
                                    local_col_ix
                                )
                            })?;

                        if global_ix < requested_column_mask.len() {
                            requested_column_mask[global_ix] = true;
                        }
                    }
                    _ => {}
                }
            }

            if enable_all_columns {
                for &global_ix in self.columns.plugin_columns(plugin_ix) {
                    if global_ix < requested_column_mask.len() {
                        requested_column_mask[global_ix] = true;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn run_plugins_streaming(
        &self,
        items: &[RepoWorkItem],
        selected_plugin_indexes: &[PluginIx],
        configs: &[PluginConfig],
        requested_column_mask: &[bool],
        rows: &mut [OutputRow],
        microbatch_rows: usize,
    ) -> PluginResult<()> {
        for &plugin_ix in selected_plugin_indexes {
            let plugin = self
                .plugins
                .get(plugin_ix)
                .ok_or_else(|| format!("unknown plugin index: {plugin_ix}"))?;
            let cfg = configs
                .get(plugin_ix)
                .ok_or_else(|| format!("missing config for plugin index: {plugin_ix}"))?;

            if !cfg.enabled {
                continue;
            }

            let requested_columns =
                self.requested_columns_for_plugin(plugin_ix, requested_column_mask);
            if requested_columns.is_empty() {
                continue;
            }

            let applicable_items: Vec<RepoWorkItem> = items
                .iter()
                .filter(|item| plugin.applies_to(&item.repo))
                .cloned()
                .collect();
            if applicable_items.is_empty() {
                continue;
            }

            let req = CollectRequest {
                items: &applicable_items,
                cfg,
                requested_columns: &requested_columns,
                microbatch_rows,
            };

            let mut stream = plugin.collect_stream(req);
            while let Some(batch_result) = stream.next().await {
                let batch = batch_result?;
                self.merge_batch(rows, plugin_ix, batch)?;
            }
        }

        Ok(())
    }

    fn requested_columns_for_plugin(
        &self,
        plugin_ix: PluginIx,
        requested_column_mask: &[bool],
    ) -> Vec<(usize, ColumnIx)> {
        self.columns
            .plugin_columns(plugin_ix)
            .iter()
            .enumerate()
            .map(|(local, &ix)| (local, ix))
            .filter(|&(_, ix)| requested_column_mask.get(ix).copied().unwrap_or(false))
            .collect()
    }

    fn merge_batch(
        &self,
        rows: &mut [OutputRow],
        plugin_ix: PluginIx,
        batch: MicroBatch,
    ) -> PluginResult<()> {
        for patch in batch {
            self.merge_patch(rows, plugin_ix, patch)?;
        }
        Ok(())
    }

    fn merge_patch(
        &self,
        rows: &mut [OutputRow],
        plugin_ix: PluginIx,
        patch: RowPatch,
    ) -> PluginResult<()> {
        let row = rows
            .get_mut(patch.row_id)
            .ok_or_else(|| format!("unknown row id: {}", patch.row_id))?;

        if row.cells.len() != self.columns.len() {
            return Err(format!(
                "row cell width mismatch: expected {}, got {}",
                self.columns.len(),
                row.cells.len()
            ));
        }

        for (column_ix, value) in patch.updates {
            let spec = self
                .columns
                .columns
                .get(column_ix)
                .ok_or_else(|| format!("unknown column index: {column_ix}"))?;
            if spec.owner_plugin_ix != plugin_ix {
                return Err(format!(
                    "plugin index {} attempted to write non-owned column '{}'",
                    plugin_ix, spec.key
                ));
            }

            row.cells[column_ix] = value;
        }

        Ok(())
    }
}

fn make_arg_long(plugin_id: &str, kind: ArgKind, decls: &[ColumnDecl]) -> String {
    match kind {
        ArgKind::PluginToggle { .. } => plugin_id.to_string(),
        ArgKind::ColumnToggle { local_col_ix, .. } => {
            let decl = decls
                .get(local_col_ix)
                .unwrap_or_else(|| {
                    panic!(
                        "invalid local_col_ix {} for plugin '{}'",
                        local_col_ix, plugin_id
                    )
                });
            format!("{plugin_id}-{}", decl.key)
        }
        ArgKind::Flag { suffix, .. }
        | ArgKind::String { suffix, .. }
        | ArgKind::StringList { suffix, .. }
        | ArgKind::Count { suffix, .. } => format!("{plugin_id}-{suffix}"),
    }
}

fn build_clap_arg(entry: &ArgEntry) -> Arg {
    let id: &'static str = Box::leak(entry.clap_id.clone().into_boxed_str());
    let long: &'static str = Box::leak(entry.long.clone().into_boxed_str());

    match entry.kind {
        ArgKind::PluginToggle { help } | ArgKind::ColumnToggle { help, .. } => {
            Arg::new(id)
                .long(long)
                .help(help)
                .action(ArgAction::SetTrue)
        }
        ArgKind::Flag { help, .. } => Arg::new(id)
            .long(long)
            .help(help)
            .action(ArgAction::SetTrue),
        ArgKind::String {
            help,
            value_name,
            default,
            ..
        } => {
            let mut arg = Arg::new(id)
                .long(long)
                .help(help)
                .value_name(value_name)
                .action(ArgAction::Set);
            if let Some(default_value) = default {
                arg = arg.default_value(default_value);
            }
            arg
        }
        ArgKind::StringList {
            help, value_name, ..
        } => Arg::new(id)
            .long(long)
            .help(help)
            .value_name(value_name)
            .action(ArgAction::Append),
        ArgKind::Count { help, .. } => Arg::new(id)
            .long(long)
            .help(help)
            .action(ArgAction::Count),
    }
}

fn parse_arg_value(matches: &ArgMatches, entry: &ArgEntry) -> ArgValue {
    match entry.kind {
        ArgKind::PluginToggle { .. } | ArgKind::ColumnToggle { .. } => {
            ArgValue::Bool(matches.get_flag(&entry.clap_id))
        }
        ArgKind::Flag { default, .. } => {
            let value = matches.get_flag(&entry.clap_id);
            ArgValue::Bool(value || default)
        }
        ArgKind::String { default, .. } => {
            let value = matches
                .get_one::<String>(&entry.clap_id)
                .cloned()
                .or_else(|| default.map(str::to_string));
            ArgValue::String(value)
        }
        ArgKind::StringList { .. } => {
            let values = matches
                .get_many::<String>(&entry.clap_id)
                .map(|it| it.cloned().collect())
                .unwrap_or_else(Vec::new);
            ArgValue::Strings(values)
        }
        ArgKind::Count { .. } => ArgValue::Count(matches.get_count(&entry.clap_id)),
    }
}

pub fn default_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new(Box::new(CoreRepoProbe));
    registry.register(Box::new(JjPlugin));
    registry.register(Box::new(BeadsPlugin));
    registry
}

struct JjPlugin;

const JJ_COL_AHEAD: usize = 0;

const JJ_COLUMNS: &[ColumnDecl] = &[ColumnDecl {
    key: "ahead",
    title: "AHEAD",
    description: "Local commits ahead of tracked remote bookmarks",
    sortable: true,
    default_in_base_format: false,
}];

const JJ_ARGS: &[ArgKind] = &[
    ArgKind::PluginToggle {
        help: "Enable Jujutsu plugin columns",
    },
    ArgKind::ColumnToggle {
        local_col_ix: JJ_COL_AHEAD,
        help: "Enable Jujutsu ahead column",
    },
];

#[derive(Clone, Copy)]
struct JjSubProcessor {
    local_col_ix: usize,
    collect_cell: fn(&RepoWorkItem) -> CellValue,
}

const JJ_SUBPROCESSORS: &[JjSubProcessor] = &[JjSubProcessor {
    local_col_ix: JJ_COL_AHEAD,
    collect_cell: jj_collect_ahead,
}];

fn jj_collect_ahead(item: &RepoWorkItem) -> CellValue {
    get_ahead(&item.path, &item.repo)
        .map(CellValue::Number)
        .unwrap_or(CellValue::Empty)
}

fn collect_jj_subprocessor_batches(
    req: &CollectRequest<'_>,
    global_col_ix: ColumnIx,
    processor: JjSubProcessor,
) -> MicroBatch {
    req.items
        .iter()
        .map(|item| RowPatch {
            row_id: item.row_id,
            updates: vec![(global_col_ix, (processor.collect_cell)(item))],
        })
        .collect()
}

fn microbatch_rows(patches: MicroBatch, size: usize) -> Vec<MicroBatch> {
    let batch_size = size.max(1);
    let mut out = Vec::new();
    let mut current = Vec::with_capacity(batch_size);

    for patch in patches {
        current.push(patch);
        if current.len() >= batch_size {
            out.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        out.push(current);
    }

    out
}

impl DetectorPlugin for JjPlugin {
    fn id(&self) -> &'static str {
        "jj"
    }

    fn description(&self) -> &'static str {
        "Jujutsu repository metrics"
    }

    fn column_decls(&self) -> &'static [ColumnDecl] {
        JJ_COLUMNS
    }

    fn arg_kinds(&self) -> &'static [ArgKind] {
        JJ_ARGS
    }

    fn configure(&self, _args: PluginArgs<'_>) -> PluginConfig {
        PluginConfig::enabled()
    }

    fn applies_to(&self, repo: &RepoInfo) -> bool {
        repo.repo_type == RepoType::Jujutsu
    }

    fn collect_stream<'a>(&'a self, req: CollectRequest<'a>) -> BatchStream<'a> {
        let mut selected_global_by_local = vec![None; JJ_COLUMNS.len()];
        for (local_col_ix, global_col_ix) in req.requested_columns {
            if *local_col_ix < selected_global_by_local.len() {
                selected_global_by_local[*local_col_ix] = Some(*global_col_ix);
            }
        }

        let mut microbatches = Vec::new();
        for processor in JJ_SUBPROCESSORS {
            let Some(global_col_ix) = selected_global_by_local[processor.local_col_ix] else {
                continue;
            };

            let patches = collect_jj_subprocessor_batches(&req, global_col_ix, *processor);
            let batches = microbatch_rows(patches, req.microbatch_rows);
            for batch in batches {
                microbatches.push(Ok(batch));
            }
        }

        Box::pin(stream::iter(microbatches))
    }
}

struct BeadsPlugin;

const BEADS_COL_LAST_CHANGED: usize = 0;

const BEADS_COLUMNS: &[ColumnDecl] = &[ColumnDecl {
    key: "beads-last-changed",
    title: "BEADS_LAST_CHANGED",
    description: "Timestamp of the most recently updated beads issue",
    sortable: true,
    default_in_base_format: false,
}];

const BEADS_ARGS: &[ArgKind] = &[
    ArgKind::PluginToggle {
        help: "Enable Beads plugin columns",
    },
    ArgKind::ColumnToggle {
        local_col_ix: BEADS_COL_LAST_CHANGED,
        help: "Enable beads last-changed column",
    },
];

#[derive(Clone, Copy)]
struct BeadsSubProcessor {
    local_col_ix: usize,
    collect_cell: fn(&RepoWorkItem) -> CellValue,
}

const BEADS_SUBPROCESSORS: &[BeadsSubProcessor] = &[BeadsSubProcessor {
    local_col_ix: BEADS_COL_LAST_CHANGED,
    collect_cell: beads_collect_last_changed,
}];

fn beads_collect_last_changed(item: &RepoWorkItem) -> CellValue {
    get_beads_last_changed(&item.path)
        .map(CellValue::Text)
        .unwrap_or(CellValue::Empty)
}

fn collect_beads_subprocessor_batches(
    req: &CollectRequest<'_>,
    global_col_ix: ColumnIx,
    processor: BeadsSubProcessor,
) -> MicroBatch {
    req.items
        .iter()
        .map(|item| RowPatch {
            row_id: item.row_id,
            updates: vec![(global_col_ix, (processor.collect_cell)(item))],
        })
        .collect()
}

impl DetectorPlugin for BeadsPlugin {
    fn id(&self) -> &'static str {
        "beads"
    }

    fn description(&self) -> &'static str {
        "Beads issue tracker metrics"
    }

    fn column_decls(&self) -> &'static [ColumnDecl] {
        BEADS_COLUMNS
    }

    fn arg_kinds(&self) -> &'static [ArgKind] {
        BEADS_ARGS
    }

    fn configure(&self, _args: PluginArgs<'_>) -> PluginConfig {
        PluginConfig::enabled()
    }

    fn applies_to(&self, _repo: &RepoInfo) -> bool {
        true
    }

    fn collect_stream<'a>(&'a self, req: CollectRequest<'a>) -> BatchStream<'a> {
        let mut selected_global_by_local = vec![None; BEADS_COLUMNS.len()];
        for (local_col_ix, global_col_ix) in req.requested_columns {
            if *local_col_ix < selected_global_by_local.len() {
                selected_global_by_local[*local_col_ix] = Some(*global_col_ix);
            }
        }

        let mut microbatches = Vec::new();
        for processor in BEADS_SUBPROCESSORS {
            let Some(global_col_ix) = selected_global_by_local[processor.local_col_ix] else {
                continue;
            };

            let patches =
                collect_beads_subprocessor_batches(&req, global_col_ix, *processor);
            let batches = microbatch_rows(patches, req.microbatch_rows);
            for batch in batches {
                microbatches.push(Ok(batch));
            }
        }

        Box::pin(stream::iter(microbatches))
    }
}
