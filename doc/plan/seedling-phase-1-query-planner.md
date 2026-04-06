# Seedling: Phase 1 Query Planner Design

Plan name: **Seedling**.

Seedling is Phase 1 of the planner/pushdown roadmap from [`/doc/planner-pushdown.md`](/doc/planner-pushdown.md). It introduces planner metadata and rule evaluation without changing final runtime behavior.

Related references:

- [`/doc/planner-pushdown.md`](/doc/planner-pushdown.md)
- [`/doc/pick-iter.md`](/doc/pick-iter.md)
- [`/src/main.rs`](/src/main.rs)
- [`/src/plugin.rs`](/src/plugin.rs)

## Why this phase exists

Current fast wins (for example `--all --format directory`) are implemented as direct special cases in [`/src/main.rs`](/src/main.rs). Seedling generalizes this into a planner that can decide when fast paths are valid.

This keeps optimization logic centralized and predictable.

## Goals

- Introduce a `LogicalQuery -> PhysicalPlan` conversion path.
- Classify known columns by availability stage and cost.
- Apply projection/filter/sort pushdown rules in planning only.
- Emit a deterministic `PhysicalPlan` object that runtime can consume later.
- Preserve current runtime behavior by default (planner can be sidecar at first).

## Non-goals

- No runtime stage executor yet (that is Flowline / Phase 2).
- No plugin API changes yet.
- No user-facing behavior changes except optional debug output.

## Proposed module layout

Create a planner domain (not flat):

- `src/planner/mod.rs` — public planner API (`build_plan`)
- `src/planner/query.rs` — `LogicalQuery`, sort/filter parsing structures
- `src/planner/meta.rs` — column capability metadata map
- `src/planner/rules.rs` — pushdown rule evaluation
- `src/planner/plan.rs` — `PhysicalPlan` stage requirements

Integration point in existing runtime:

- `src/main.rs` calls `planner::build_plan(...)` after argument parsing.

## Data contracts

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecMode {
    Full,
    Scan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvailabilityStage {
    Enumerate,
    EarlyProbe,
    LateProbe,
    Finalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostClass {
    Free,
    Cheap,
    Expensive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnPlanMeta {
    pub key: &'static str,
    pub stage: AvailabilityStage,
    pub cost: CostClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalQuery {
    pub mode: ExecMode,
    pub roots: Vec<std::path::PathBuf>,
    pub projection: Vec<String>,
    pub filters: Vec<FilterExpr>,
    pub sort_keys: Vec<SortKey>,
    pub emit_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalPlan {
    pub required_columns: Vec<String>,
    pub earliest_render_stage: AvailabilityStage,
    pub needs_late_probe: bool,
    pub can_stream_early: bool,
    pub fast_path: FastPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastPath {
    None,
    DirectoryOnly,
}
```

## Initial column capability table

Seedling uses a hardcoded map in `src/planner/meta.rs`:

| Column | Stage | Cost |
|---|---|---|
| `directory` | `Enumerate` | `Free` |
| `status` | `EarlyProbe` | `Cheap` |
| `change-date` | `EarlyProbe` | `Cheap` |
| `workparent` | `LateProbe` | `Cheap` |
| `commit-date` | `LateProbe` | `Expensive` |
| `ahead` | `LateProbe` | `Expensive` |

Unknown columns default to `LateProbe` + `Expensive` (safe fallback).

## Planning rules

### Projection pushdown

`required_columns` is the union of:

- projection keys
- filter-referenced keys
- sort keys

If the union is exactly `{directory}`, planner returns `FastPath::DirectoryOnly`.

### Filter stage assignment

Each filter is assigned to the earliest stage where its column is available.

- early filters can reduce rows before expensive work
- late filters are marked and deferred

### Sort stage assignment

Sort keys are split by stage:

- early-sort keys (`Enumerate`/`EarlyProbe`)
- final-sort keys (`LateProbe`)

Planner marks whether staged sort is needed in Phase 2.

### Mode + policy decision

For `ExecMode::Scan`, late requirements are recorded so runtime can decide policy in Phase 2 (`upgrade`, `defer`, `error`).

Seedling does not enforce the policy; it only computes requirements.

## Integration sequence

1. Parse CLI args in [`/src/main.rs`](/src/main.rs).
2. Build `LogicalQuery` from parsed args.
3. Call `planner::build_plan(&query)`.
4. Use plan for diagnostics and fast-path selection (initially `DirectoryOnly`).
5. Continue existing runtime path unchanged for non-fast-path cases.

This gives us planner coverage without runtime churn.

## Acceptance criteria

- `planner::build_plan` exists and is covered by unit tests.
- Planner metadata table exists for all currently documented output columns.
- Directory-only requests (`directory` or `{directory}`) resolve to `FastPath::DirectoryOnly`.
- Mixed projection/sort/filter queries produce deterministic `required_columns` regardless of input ordering.
- Scan queries with late keys are detected and marked in the plan.
- Existing output behavior remains unchanged for non-fast-path queries.

## Verification

- Unit tests in planner module for:
  - projection union logic
  - fast-path detection
  - sort/filter stage assignment
  - unknown column fallback behavior
- CLI smoke checks still pass for:
  - `is-tree --all --format directory`
  - `is-tree --all --format "{status} {directory}"`
  - `is-tree --all --format all --json`

## Risks and mitigations

- **Risk:** planner and runtime diverge.
  - **Mitigation:** keep planner outputs explicit and tested; only consume planner decisions through well-defined fields.
- **Risk:** unknown columns accidentally break planning.
  - **Mitigation:** safe fallback to late/expensive.
- **Risk:** overfitting to current columns.
  - **Mitigation:** isolate metadata table so plugin metadata can replace it in a later phase.

## Done when

Seedling is complete when `is-tree` has a test-backed planning layer that can accurately identify fast-path and stage requirements, while preserving all existing runtime behavior.
