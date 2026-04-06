# Planner + Pushdown Engine for `is-tree`

This document proposes a lightweight query planner for `is-tree` so we can push computation earlier in the pipeline, especially for `--scan` and picker workflows.

Related docs and code:

- [`/doc/pick-iter.md`](/doc/pick-iter.md)
- [`/README.md`](/README.md)
- [`/src/main.rs`](/src/main.rs)
- [`/src/plugin.rs`](/src/plugin.rs)

## Why now

We already landed one targeted optimization: short-circuiting `--all --format directory` in [`/src/main.rs`](/src/main.rs).

That win validates the direction, but it is still a special case. We need a general mechanism that can answer:

- Which columns are actually needed?
- Which filters can run before expensive plugin work?
- Which sorts can run early enough to keep `--scan` responsive?

In short: turn the CLI request into an execution plan, then push expensive work as late as possible.

## Goals

- Preserve current CLI behavior by default.
- Make fast paths automatic when query shape allows.
- Keep `--scan` interactive by prioritizing early-sort keys.
- Reuse existing plugin registry architecture instead of bypassing it.
- Allow incremental rollout without rewriting the whole runtime.

## Non-goals

- No SQL parser or user-facing query DSL.
- No distributed execution.
- No breaking changes to existing output formats.

## Core idea

Treat each invocation as a query:

- **Projection**: requested output columns
- **Filters**: row predicates
- **Sort**: ordered keys
- **Mode**: full vs scan
- **Input**: explicit paths or discovered roots

Then compile to a physical plan where each column and predicate is annotated by when it becomes available and how expensive it is.

## Architecture

```mermaid
flowchart LR
    ParseCli[Parse CLI Args] --> LogicalQuery[Build LogicalQuery]
    LogicalQuery --> PlanRules[Apply Pushdown Rules]
    PlanRules --> PhysicalPlan[Build PhysicalPlan]
    PhysicalPlan --> EnumerateStage[Enumerate Candidate Paths]
    EnumerateStage --> EarlyProbeStage[Run Early Probes]
    EarlyProbeStage --> EarlyFilterSort[Apply Early Filters and Sorts]
    EarlyFilterSort --> LateProbeStage[Run Late Plugin Probes If Required]
    LateProbeStage --> FinalFilterSort[Apply Remaining Filters and Sorts]
    FinalFilterSort --> RenderStage[Render Text or JSON]
```

## Data model draft

These are implementation-level structs we can add near runtime planning code.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecMode {
    Full,
    Scan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogicalQuery {
    mode: ExecMode,
    roots: Vec<std::path::PathBuf>,
    projection: Vec<String>,
    filters: Vec<FilterExpr>,
    sort_keys: Vec<SortKey>,
    emit_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SortKey {
    column: String,
    desc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AvailabilityStage {
    Enumerate,
    EarlyProbe,
    LateProbe,
    Finalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CostClass {
    Free,
    Cheap,
    Expensive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColumnPlanMeta {
    key: &'static str,
    stage: AvailabilityStage,
    cost: CostClass,
    stable_in_scan: bool,
}
```

### Practical column classification (initial)

| Column | Stage | Cost | Notes |
|---|---|---|---|
| `directory` | `Enumerate` | `Free` | Known from input path list |
| `status` | `EarlyProbe` | `Cheap` | `detect_repo(path)` is local fs checks |
| `workparent` | `LateProbe` | `Cheap` | Path parsing + repo metadata checks |
| `change-date` | `EarlyProbe` | `Cheap` | local metadata mtime |
| `commit-date` | `LateProbe` | `Expensive` | subprocess/git history lookup |
| `ahead` | `LateProbe` | `Expensive` | jj/git remote-related logic |

This table is the planner contract. It can start hard-coded and later move to plugin metadata.

## Pushdown rules

### 1) Projection pushdown

Only compute columns that are needed by:

- output projection
- filter predicates
- sort keys

If requested columns are only `directory`, skip repo probe and plugins entirely.

### 2) Filter pushdown

Apply predicates at earliest available stage.

Examples:

- `status == jj` can run at `EarlyProbe`.
- `ahead > 0` must wait for `LateProbe`.

### 3) Sort pushdown

Sort as early as possible, but only when sort keys are available.

- `--sort directory+` sorts during enumeration.
- `--sort change-date-` sorts after `EarlyProbe`.
- `--sort ahead-` requires `LateProbe`.

If multiple keys are mixed, planner splits sort into staged ordering:

- Early stable sort on early keys
- Final sort after late keys are available

### 4) Mode-aware gating

`--scan` should avoid `LateProbe` by default.

Planner behavior in scan mode:

- If query needs only `Enumerate`/`EarlyProbe` columns, stay scan-fast.
- If query requests late columns or late sort keys, use policy:
  - `upgrade`: automatically switch to full plan
  - `defer`: keep scan-fast behavior and warn that late requirements are skipped
  - `error`: fail with clear message

Default recommendation: `upgrade` for correctness unless user opts into strict fast mode.

## Execution examples

### Case A: `--all --format directory`

Plan:

1. Enumerate candidate subdirectories
2. Render path list

No probe, no plugin execution.

### Case B: `--scan --format "{status} {directory}" --sort directory+`

Plan:

1. Enumerate
2. Early probe for `status`
3. Early sort by `directory`
4. Stream render

No late stage required.

### Case C: `--scan --sort ahead- --format directory`

Planner detects `ahead` as late/expensive.

- With `upgrade`: switch to full mode and compute ahead before final sort.
- With `defer`: run scan-only path ordering and warn that `ahead` sort is not applied.

## Integration with picker pipeline

This planner directly supports the high-value pipeline from [`/doc/pick-iter.md`](/doc/pick-iter.md):

```bash
is-tree --scan --sort change-date- --format directory | fuzzel --dmenu --multi | is-tree --stdin --format all
```

Key benefits:

- Fast candidate emission (`Enumerate` + `EarlyProbe` only)
- Useful prioritization (`change-date` pushdown)
- Expensive columns deferred until user has narrowed selection

## Implementation plan

### Phase 1: planner metadata and rule engine

- Add `LogicalQuery`, `PhysicalPlan`, and column metadata table.
- Build planner from existing CLI args (`format`, `sort`, `filter`, `json`, `all`).
- Keep old runtime path as fallback.

Acceptance:

- Planner returns deterministic stage assignment for projection/filter/sort keys.
- `--all --format directory` is represented as enumerate-only plan.

### Phase 2: staged execution runtime

- Introduce execution stages in `run()` path:
  - enumerate
  - early probe/filter/sort
  - optional late probe/filter/sort
  - render
- Route current short-circuit through planner instead of bespoke branch.

Acceptance:

- Existing directory-only optimization remains fast and behaviorally identical.
- Query results remain equivalent to current behavior for full-mode queries.

### Phase 3: scan policy + diagnostics

- Add scan late-key policy (`upgrade`, `defer`, `error`).
- Emit explicit diagnostics when requested sort/filter cannot run in scan-fast stage.

Acceptance:

- Users can predictably control correctness vs speed in scan mode.
- Help text documents scan policy behavior.

### Phase 4: plugin metadata integration

- Extend plugin column declarations with planning hints (`stage`, `cost`).
- Remove hard-coded planner map once plugin hints are complete.

Acceptance:

- Planner decisions come from plugin metadata rather than ad-hoc key matching.
- New plugins can participate in pushdown automatically.

## Testing strategy

- Unit tests for planner rule decisions:
  - projection-only query
  - mixed early/late sort keys
  - scan policy behaviors
- Integration tests for runtime equivalence:
  - full mode unchanged output
  - scan mode staged behavior
- Performance checks:
  - compare current vs planned execution on large directory sets

## Ticket alignment

- `is-tree-scan-priority`: provides the mechanism to prioritize and stream candidates.
- `is-tree-fuzzel-pipeline`: provides the UX workflow that consumes staged scan output.
- `is-tree-per-file-stats` and `is-tree-staleness-views`: benefit from selecting expensive drill-down only after narrowing candidates.

## Decision summary

We should evolve `is-tree` from ad-hoc fast paths into a small planner-driven runtime:

- classify column availability/cost
- push projection/filter/sort as early as possible
- keep `--scan` responsive while preserving correctness controls

This gives us a reusable optimization model, not just one-off special cases.
