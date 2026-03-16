use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;

use clap::{Arg, ArgMatches, Command};
use futures_core::Stream;
use futures_util::StreamExt;

use crate::detect::{detect_repo, RepoInfo};

pub type RowId = usize;
pub type PluginIx = usize;
pub type ColumnIx = usize;

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

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
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

#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub plugin_id: &'static str,
    pub arg: Arg,
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
    pub requested_columns: &'a [ColumnIx],
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

    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn configure(&self, _matches: &ArgMatches) -> PluginConfig {
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
}

impl PluginRegistry {
    pub fn new(repo_probe: Box<dyn RepoProbe>) -> Self {
        Self {
            repo_probe,
            plugins: Vec::new(),
            plugin_by_id: HashMap::new(),
            columns: ColumnCatalog::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn DetectorPlugin>) {
        let plugin_id = plugin.id();
        if self.plugin_by_id.contains_key(plugin_id) {
            panic!("duplicate plugin id: {plugin_id}");
        }

        let plugin_ix = self.plugins.len();
        self.columns.register_plugin_slot();
        for decl in plugin.column_decls() {
            self.columns.push_column(plugin_ix, *decl);
        }

        self.plugin_by_id.insert(plugin_id, plugin_ix);
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
                    panic!("duplicate clap arg id '{arg_id}' from plugin '{}'", plugin.id());
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

    pub fn configure_all(&self, matches: &ArgMatches) -> Vec<PluginConfig> {
        let mut configs = Vec::with_capacity(self.plugins.len());
        for plugin in &self.plugins {
            configs.push(plugin.configure(matches));
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

            let requested_columns = self.requested_columns_for_plugin(plugin_ix, requested_column_mask);
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
    ) -> Vec<ColumnIx> {
        self.columns
            .plugin_columns(plugin_ix)
            .iter()
            .copied()
            .filter(|&ix| requested_column_mask.get(ix).copied().unwrap_or(false))
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

pub fn default_registry() -> PluginRegistry {
    PluginRegistry::new(Box::new(CoreRepoProbe))
}
