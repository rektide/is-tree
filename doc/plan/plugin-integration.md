# Plugin Integration Plan

This plan integrates the new indexed plugin architecture into runtime execution while preserving current CLI behavior.

Related references:

- [`/doc/plugin.md`](/doc/plugin.md)
- [`/src/plugin.rs`](/src/plugin.rs)
- [`/src/main.rs`](/src/main.rs)

## Objectives

- Make plugin execution the primary data path.
- Preserve current output and flag behavior during migration.
- Keep compatibility for existing workflows while introducing plugin-owned columns and args.
- Move to async execution for plugin streaming and microbatches.

## Current State

- The plugin engine exists in [`/src/plugin.rs`](/src/plugin.rs), including indexed columns, arg catalogs, and streaming merge.
- `jj` exists as a plugin with sub-processor structure.
- Runtime still uses the legacy fixed detector pipeline in [`/src/main.rs`](/src/main.rs).

## Target Architecture

```mermaid
flowchart LR
    Parse[Base CLI + Plugin Args] --> Probe[Probe RepoWorkItem list]
    Probe --> Resolve[Resolve requested columns + plugin selection]
    Resolve --> Stream[Run plugin streams]
    Stream --> Merge[Merge RowPatch microbatches]
    Merge --> Adapt[Adapt OutputRow for rendering]
    Adapt --> Render[Filter + Sort + Format/JSON]
```

## Integration Phases

### 1) Compose CLI Through Registry

- Replace `Cli::parse()`-only execution with command composition:
  - build base command in [`/src/main.rs`](/src/main.rs)
  - inject plugin args via `registry.build_command(...)`
  - parse into `ArgMatches`
- Keep existing base options and behavior intact.

Acceptance criteria:

- `--help` includes both base flags and plugin flags.
- Existing base invocations still parse exactly as before.

### 2) Add Compatibility Plugins for Existing Columns

- Implement plugin wrappers for currently hardcoded columns:
  - `status`, `directory`
  - `commit-date`, `change-date`
  - `workparent`, `variant`
  - `beads`
- Keep existing detection helpers as implementation internals during this phase.

Acceptance criteria:

- `--format all` can be generated from registry columns.
- Column ownership is unique and validated at registry startup.

### 3) Define Enablement Semantics

- Centralize policy for plugin/column enablement:
  - format-requested columns are always active
  - plugin toggle enables all plugin-owned columns
  - column toggle enables one column
  - legacy flags map to equivalent plugin toggles
- Ensure deterministic precedence when both legacy and plugin flags are used.

Acceptance criteria:

- Documented precedence and explicit tests for edge cases.
- No ambiguity in resolved requested column mask.

### 4) Wire Runtime to Plugin Data Path

- Build `RepoWorkItem` list from input paths.
- Resolve selected plugins and requested column mask.
- Run `run_plugins_streaming(...)` to populate rows.
- Adapt plugin `OutputRow` into current rendering structures as temporary bridge.

Acceptance criteria:

- Default output path no longer calls per-column detector functions directly.
- Output for baseline scenarios remains equivalent to legacy behavior.

### 5) Switch to Async Main Path

- Convert main entrypoint to async runtime (`tokio`).
- Await plugin streaming pipeline in the main execution path.
- Keep synchronous helper wrappers only where required, with clear follow-up to convert subprocess calls.

Acceptance criteria:

- CLI executes end-to-end through async plugin pipeline.
- No behavior regressions in output formatting and JSON mode.

### 6) Replace Legacy Pipeline and Cleanup

- Remove obsolete detector wiring in [`/src/main.rs`](/src/main.rs) once parity is proven.
- Remove compatibility shims that are no longer needed.
- Keep docs and help text aligned with plugin-owned flags.

Acceptance criteria:

- Legacy fixed pipeline code path is removed.
- `main` delegates data collection entirely to plugin registry.

## Verification Strategy

- Snapshot representative CLI outputs before and after integration:
  - default output
  - `--format` custom columns
  - `--format all`
  - `--json`
  - filter and sort combinations
- Add tests for enablement semantics and column resolution.
- Add tests for plugin merge safety:
  - unknown row id rejection
  - non-owner column write rejection

## Risks and Mitigations

- **Flag behavior drift:** mitigate with compatibility mapping and parse tests.
- **Output drift during migration:** mitigate with snapshot tests and adapter bridge.
- **Concurrency nondeterminism:** keep stable row ordering and deterministic merge semantics.
- **Performance regressions:** use microbatch sizing and plugin-local batching; add perf checks around large directory sets.

## Completion Criteria

- Plugin registry is the single source of truth for columns and plugin args.
- Runtime data collection uses async plugin streaming only.
- Existing user-facing behavior is preserved or intentionally documented where changed.
