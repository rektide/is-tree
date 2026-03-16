# Plugin Architecture Draft

This document proposes type definitions and a registry layout for detector plugins.

Current wiring is centralized in [`/src/main.rs`](/src/main.rs) and detector helpers exported from [`/src/detect/mod.rs`](/src/detect/mod.rs). The goal is to preserve current behavior while enabling detectors to advertise their own flags and output columns.

## Goals

- Keep repository probing (`git`/`jj`/worktree/none) as a single shared step.
- Let plugins declare:
  - CLI flags,
  - output columns,
  - applicability constraints (repo type/worktree),
  - data collection behavior.
- Build `--format all` and column help from plugin metadata instead of hardcoding.
- Run only selected/needed plugins to avoid unnecessary shell calls.

## Proposed Core Type Definitions

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use clap::{Arg, ArgMatches, Command};

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

#[derive(Debug, Clone)]
pub struct DetectionCtx<'a> {
    pub path: &'a Path,
    pub repo: &'a RepoInfo,
    pub now_unix_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(pub &'static str);

#[derive(Debug, Clone)]
pub enum CellValue {
    Text(String),
    Number(isize),
    Empty,
}

#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub id: ColumnId,                 // e.g. "status", "ahead", "beads"
    pub title: &'static str,          // e.g. "STATUS", "AHEAD"
    pub description: &'static str,
    pub sortable: bool,
    pub default_in_base_format: bool, // true for status/directory
}

#[derive(Debug, Clone)]
pub struct OutputRow {
    pub path: PathBuf,
    pub cells: BTreeMap<ColumnId, CellValue>,
}

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

pub trait RepoProbe: Send + Sync {
    fn id(&self) -> &'static str; // typically "core-repo-probe"
    fn detect(&self, path: &Path) -> RepoInfo;
}

pub trait DetectorPlugin: Send + Sync {
    fn id(&self) -> &'static str; // unique, stable, cli-safe: "jj-ahead"
    fn description(&self) -> &'static str;

    // columns this plugin can populate
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

    // quick applicability check before work
    fn applies_to(&self, repo: &RepoInfo) -> bool;

    // write zero or more output cells
    fn collect(&self, ctx: &DetectionCtx<'_>, cfg: &PluginConfig, row: &mut OutputRow);
}
```

### Notes on the typedefs

- `ColumnId` is string-backed to keep custom format strings ergonomic (`{ahead}`, `{beads}`, etc.).
- `CellValue` allows typed sorting/JSON emission while still supporting plain text formatting.
- `ArgSpec` wraps `clap::Arg` so each plugin can advertise flags without owning the full `Command`.
- `PluginConfig` is intentionally generic; plugins can deserialize plugin-local options into richer local types internally.

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
        // Return composed command.
        cmd
    }

    pub fn configure_all(&self, matches: &ArgMatches)
        -> BTreeMap<&'static str, PluginConfig>
    {
        // plugin_id -> parsed plugin config
        BTreeMap::new()
    }

    pub fn columns(&self) -> &[ColumnSpec] {
        &self.all_columns
    }

    pub fn run_plugins(
        &self,
        ctx: &DetectionCtx<'_>,
        selected_plugins: &BTreeSet<&'static str>,
        configs: &BTreeMap<&'static str, PluginConfig>,
        row: &mut OutputRow,
    ) {
        // deterministic plugin order (registration order)
        // skip if not selected or not applicable
        // plugin.collect(...)
    }
}
```

### Registry invariants

- Plugin IDs are unique.
- Column IDs are unique across all plugins.
- CLI argument IDs/long names are unique after plugin injection.
- Registration order is execution order.

## Suggested Built-in Registry Entries

This keeps behavior aligned with current detector functions under [`/src/detect`](/src/detect).

```rust
pub fn default_registry() -> PluginRegistry {
    let mut reg = PluginRegistry::new(Box::new(CoreRepoProbe));

    reg.register(Box::new(CoreStatusPlugin));     // status, directory
    reg.register(Box::new(DatesPlugin));          // commit-date, change-date
    reg.register(Box::new(WorkparentPlugin));     // workparent
    reg.register(Box::new(VariantPlugin));        // variant
    reg.register(Box::new(JjAheadPlugin));        // ahead
    reg.register(Box::new(BeadsPrefixPlugin));    // beads

    reg
}
```

## CLI Flag Ownership Model

Core flags remain global:

- `--all`, positional directories
- `--filter`, `--sort`, `--format`, `--json`, `--header`, `--separator`

Plugin flags are plugin-owned and namespaced:

- `--dates` / `--commit-date` / `--change-date` (dates plugin)
- `--jj-ahead` and optional future tuning flags (jj-ahead plugin)
- `--beads` and optional future source flags (beads plugin)

Recommended naming rule:

- Plugin IDs are kebab-case.
- Plugin-private flags are prefixed with plugin ID when not already historical (`--jj-ahead-max`, `--beads-timeout-ms`).
- Keep backward-compatible aliases during migration.

## Execution Wiring

```rust
// 1) Build registry
let registry = default_registry();

// 2) Build clap command = base + plugin args
let cmd = registry.build_command(base_command());
let matches = cmd.get_matches();

// 3) Configure plugins once
let configs = registry.configure_all(&matches);

// 4) For each path
for path in paths {
    let repo = registry.repo_probe.detect(&path);
    let ctx = DetectionCtx { path: &path, repo: &repo, now_unix_secs };

    let mut row = OutputRow {
        path: path.clone(),
        cells: BTreeMap::new(),
    };

    registry.run_plugins(&ctx, &selected_plugins, &configs, &mut row);
    rows.push(row);
}
```

## Migration Outline

1. Introduce `ColumnSpec`, `CellValue`, `OutputRow`, and `PluginRegistry` with no behavior changes.
2. Wrap existing detectors as plugins that call current helper functions.
3. Generate `--format all` from `registry.columns()`.
4. Move static clap derive to composed `clap::Command` builder.
5. Keep existing flags as aliases, then gradually shift help text to plugin ownership.

## Open Decisions

- Whether plugin failures should be silent (`None`) or include an optional diagnostics column.
- Whether plugin collection should remain fully sequential or allow optional parallel plugin execution.
- Whether to keep all columns globally sortable, or sortability-by-type (`Text` vs `Number`) only.
