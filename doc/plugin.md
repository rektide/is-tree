# Plugin Architecture (Async Microbatch Draft)

This document defines an async plugin interface and registry layout for detector plugins.

Current wiring is centralized in [`/src/main.rs`](/src/main.rs), with detector helpers exported from [`/src/detect/mod.rs`](/src/detect/mod.rs). This draft updates the prior design to support async streaming collection and domain plugins (for example, one `jj` plugin that can emit multiple columns).

## Design Changes From Prior Draft

- Plugin execution is now async.
- `collect` now streams microbatches (`Vec<RowPatch>`) instead of mutating one row directly.
- Added `RepoWorkItem` as the stable per-row unit of work (`row_id`, `path`, `repo`).
- Shifted from plugin-per-column preference to domain plugins that own multiple columns.
- Registry merging is row-patch based and deterministic.

## Goals

- Keep repository probing (`git`/`jj`/worktree/none) as a single shared step.
- Let plugins declare:
  - CLI flags,
  - owned output columns,
  - applicability constraints,
  - async collection strategy.
- Allow plugins to do bulk work and optional sub-runs while preserving per-row mapping.
- Build `--format all` and column help from plugin metadata instead of hardcoding.
- Run only selected and requested columns to avoid unnecessary subprocess calls.

## Core Types

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;

use clap::{Arg, ArgMatches, Command};
use futures_core::Stream;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepoType {
    Git,
    Jujutsu,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub repo_type: RepoType,
    pub is_worktree: bool,
}

pub type RowId = usize;

#[derive(Debug, Clone)]
pub struct RepoWorkItem {
    pub row_id: RowId,
    pub path: PathBuf,
    pub repo: RepoInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(pub &'static str);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellValue {
    Text(String),
    Number(isize),
    Empty,
}

#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub id: ColumnId,
    pub title: &'static str,
    pub description: &'static str,
    pub sortable: bool,
    pub default_in_base_format: bool,
}

#[derive(Debug, Clone)]
pub struct OutputRow {
    pub row_id: RowId,
    pub path: PathBuf,
    pub cells: BTreeMap<ColumnId, CellValue>,
}

#[derive(Debug, Clone)]
pub struct RowPatch {
    pub row_id: RowId,
    pub cells: BTreeMap<ColumnId, CellValue>,
}

pub type MicroBatch = Vec<RowPatch>;
pub type BatchStream<'a> =
    Pin<Box<dyn Stream<Item = anyhow::Result<MicroBatch>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub plugin_id: &'static str,
    pub arg: Arg,
}

#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub enabled: bool,
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CollectRequest<'a> {
    pub items: &'a [RepoWorkItem],
    pub cfg: &'a PluginConfig,
    pub requested_columns: &'a BTreeSet<ColumnId>,
    pub microbatch_rows: usize,
}
```

## Traits

```rust
pub trait RepoProbe: Send + Sync {
    fn id(&self) -> &'static str; // typically "core-repo-probe"
    fn detect(&self, path: &Path) -> RepoInfo;
}

pub trait DetectorPlugin: Send + Sync {
    fn id(&self) -> &'static str; // unique, stable, cli-safe: "jj"
    fn description(&self) -> &'static str;
    fn columns(&self) -> &'static [ColumnSpec];

    // plugin-owned arguments to inject into clap::Command
    fn args(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    // parse plugin-owned args once after global parse
    fn configure(&self, _matches: &ArgMatches) -> PluginConfig {
        PluginConfig {
            enabled: true,
            options: BTreeMap::new(),
        }
    }

    // optional coarse filter before collection
    fn applies_to(&self, _repo: &RepoInfo) -> bool {
        true
    }

    // emit zero or more microbatches of row patches
    fn collect_stream<'a>(&'a self, req: CollectRequest<'a>) -> BatchStream<'a>;
}
```

## Registry Layout

```rust
pub struct PluginRegistry {
    repo_probe: Box<dyn RepoProbe>,
    plugins: Vec<Box<dyn DetectorPlugin>>,

    // indexes built at startup
    by_id: BTreeMap<&'static str, usize>,
    column_owner: BTreeMap<ColumnId, &'static str>,
    all_columns: Vec<ColumnSpec>,
}

impl PluginRegistry {
    pub fn new(repo_probe: Box<dyn RepoProbe>) -> Self { /* ... */ }

    pub fn register(&mut self, plugin: Box<dyn DetectorPlugin>) { /* ... */ }

    pub fn build_command(&self, mut cmd: Command) -> Command {
        // Inject plugin args after base args.
        // Validate no duplicate arg IDs / long names.
        cmd
    }

    pub fn configure_all(&self, matches: &ArgMatches)
        -> BTreeMap<&'static str, PluginConfig>
    {
        BTreeMap::new()
    }

    pub fn columns(&self) -> &[ColumnSpec] {
        &self.all_columns
    }

    pub fn probe_items(&self, paths: &[PathBuf]) -> Vec<RepoWorkItem> {
        // row_id is index in this vector
        Vec::new()
    }

    pub async fn run_plugins_streaming(
        &self,
        items: &[RepoWorkItem],
        selected_plugins: &BTreeSet<&'static str>,
        configs: &BTreeMap<&'static str, PluginConfig>,
        requested_columns: &BTreeSet<ColumnId>,
        rows: &mut [OutputRow],
    ) -> anyhow::Result<()> {
        // 1) start selected plugin streams concurrently
        // 2) consume microbatches
        // 3) merge patches into rows by row_id
        // 4) enforce column ownership
        Ok(())
    }
}
```

## Registry Invariants

- Plugin IDs are unique.
- Column IDs are unique across all plugins.
- Each column has exactly one owner plugin.
- CLI argument IDs/long names are unique after plugin injection.
- Registration order is stable and used for deterministic tie-breaking.
- Patch merge must reject unknown row IDs and non-owner column writes.

## Built-in Registry Shape

Use domain-grouped plugins, not one plugin per column.

```rust
pub fn default_registry() -> PluginRegistry {
    let mut reg = PluginRegistry::new(Box::new(CoreRepoProbe));

    reg.register(Box::new(CoreStatusPlugin)); // status, directory
    reg.register(Box::new(DatesPlugin));      // commit-date, change-date
    reg.register(Box::new(WorktreePlugin));   // workparent, variant
    reg.register(Box::new(JjPlugin));         // ahead (+ future jj columns)
    reg.register(Box::new(BeadsPlugin));      // beads

    reg
}
```

`JjPlugin` can run internal sub-processors while remaining one plugin:

- `ahead_processor`
- `bookmark_processor`
- `working_copy_processor`

Each sub-processor emits patches keyed by `row_id`.

## CLI Flag Ownership Model

Core flags remain global:

- `--all`, positional directories
- `--filter`, `--sort`, `--format`, `--json`, `--header`, `--separator`

Plugin flags are plugin-owned and namespaced:

- `--jj` (enable jj plugin)
- `--jj-ahead`, `--jj-behind`, `--jj-bookmarks` (column toggles)
- `--dates`, `--commit-date`, `--change-date`
- `--beads`

Recommended naming rule:

- Plugin IDs are kebab-case.
- Plugin-private flags use plugin-prefixed names when not historical.
- Keep backward-compatible aliases during migration.

## Execution Wiring

```rust
// 1) Build registry
let registry = default_registry();

// 2) Build clap command = base + plugin args
let cmd = registry.build_command(base_command());
let matches = cmd.get_matches();

// 3) Parse plugin configs
let configs = registry.configure_all(&matches);

// 4) Build probed work items
let items = registry.probe_items(&paths);

// 5) Prepare output rows with stable row_id
let mut rows: Vec<OutputRow> = items.iter().map(|item| OutputRow {
    row_id: item.row_id,
    path: item.path.clone(),
    cells: BTreeMap::new(),
}).collect();

// 6) Compute requested columns (from --format/default/sort deps)
let requested_columns = resolve_requested_columns(&matches, registry.columns());

// 7) Run plugins concurrently and merge streamed microbatches
registry.run_plugins_streaming(
    &items,
    &selected_plugins,
    &configs,
    &requested_columns,
    &mut rows,
).await?;

// 8) Filter/sort/render
render(rows, ...);
```

## Async Runtime Notes

- Use `tokio` runtime in main.
- Prefer `tokio::process::Command` for subprocesses (for true async collection).
- Use `futures-util` stream utilities for merging plugin streams.
- Keep bounded buffering between collectors and merger to preserve backpressure.

## Migration Outline

1. Keep existing behavior while introducing `RepoWorkItem`, `RowPatch`, and async stream signatures.
2. Convert current plugin scaffolding to domain plugins (`JjPlugin`, `WorktreePlugin`, etc.).
3. Generate `--format all` from `registry.columns()`.
4. Move static clap derive to composed `clap::Command` builder.
5. Switch internal command execution to async process APIs.
6. Keep old flags as aliases until help/docs are updated.

## Improvements To Implement Next

- **Error model:** support optional diagnostics patches (non-fatal plugin errors per row/plugin).
- **Merge policy:** explicitly define overwrite behavior when a plugin emits multiple patches for the same cell.
- **Cancellation/timeouts:** add plugin-level timeout and cooperative cancellation.
- **Observability:** per-plugin metrics (batch count, rows/sec, command durations).
- **Column dependency graph:** allow a column to declare prerequisites for automatic request expansion.
- **Deterministic output under concurrency:** preserve stable row order and stable sort semantics.

## Open Decisions

- Should plugin failures be silent (`Empty`) by default, or surfaced via a diagnostics column?
- Should microbatch size be globally configured, plugin-configured, or adaptive?
- Should plugins emit partial patches for rows that are later filtered out, or should filtering happen earlier?
- Do we support cross-plugin derived columns, or require strict single-owner derivation?
