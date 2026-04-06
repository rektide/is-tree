# is-tree

> CLI utility to detect and classify Git and Jujutsu repositories

## Usage

```bash
# Test a specific directory
is-tree ~/src/compfuzor

# Test all directories in current path with --all/-a
is-tree --all

# Test specific directories by positional arguments
is-tree ~/src/compfuzor ~/src/niri-mcp

# Custom output format with interpolated columns
is-tree --all --format "{status} {directory} {ahead}"
is-tree --all --format all

# Enable jj plugin columns
is-tree --all --jj

# Output as JSON (respects --format for column selection)
is-tree --all --json
is-tree --all --format "{status} {directory} {ahead}" --json

# Header row with custom separator
is-tree --all --header --separator " | "
```

### Planned usage (not yet implemented)

```bash
# Filter for specific types (comma-separated, - for NOT)
is-tree --all --filter git
is-tree --all --filter git,jj
is-tree --all --filter worktree-
is-tree --all --filter git-,jj

# Sort by any column (use + for ascending, - for descending)
is-tree --all --sort change-date-
is-tree --all --sort workparent,directory+

# Custom date format
is-tree --all --date "%Y-%m-%d %H:%M:%S"
```

## Options

### Implemented

| Flag | Description |
|------|-------------|
| `--all`, `-a` | Scan all non-hidden subdirectories in current directory |
| `--format <template>` | Output format with `{column}` placeholders; `all` for every column |
| `--json` | Output as JSON array (respects `--format` for column selection) |
| `--header` | Print a header row above text output |
| `--separator <string>` | Replace spaces between columns in text output (default: single space) |
| `--plugins <ids>` | Comma-separated plugin ids to run (default: all) |
| `--microbatch-rows <n>` | Max row patches per streaming microbatch (default: 64) |
| `--jj` | Enable jj plugin columns (equivalent to requesting all jj columns) |
| `--jj-ahead` | Enable the `ahead` column |
| `<directories>` | Positional paths to inspect |

### Not yet implemented

| Flag | Description | Ticket |
|------|-------------|--------|
| `--filter <type>` | Filter by repo type: `git`, `jj`, `worktree`, `worktree-git`, `worktree-jj`. Suffix `-` for NOT | [is-tree-sort-filter-date] |
| `--sort <column>` | Sort by column(s). Suffix `+` ascending, `-` descending | [is-tree-sort-filter-date] |
| `--date <format>` | strftime-style date format (default: ISO 8601) | [is-tree-sort-filter-date] |

## Output

Default format: `<status> <directory>`

### Columns

Available columns for `--format`:

**Implemented:**

| Column | Description | Source |
|--------|-------------|--------|
| `status` | Repository type (git, jj, worktree-git, worktree-jj, none) | core |
| `directory` | Full directory path | core |
| `ahead` | Local JJ commits ahead of tracked remote bookmarks | `--jj` / `--jj-ahead` |

**Not yet implemented:**

| Column | Description | Ticket |
|--------|-------------|--------|
| `workparent` | Directory name of the workparent (worktrees only) | [is-tree-sort-filter-date] |
| `commit-date` | Most recent commit date | [is-tree-sort-filter-date] |
| `change-date` | Most recent file change date | [is-tree-sort-filter-date] |

### Status values
- `git` - Git repository (not Jujutsu, not a worktree)
- `jj` - Jujutsu repository (not a worktree)
- `worktree-git` - Git worktree
- `worktree-jj` - Jujutsu worktree
- `none` - Not a recognized repository

### JSON Output
When `--json` is specified, output is an array of objects. By default, includes `status` and `directory` columns. Use `--format` to control which columns appear in the JSON output.

## Detection

### Strategy Overview

The detection strategy uses a hierarchical approach to classify directories based on the presence and characteristics of version control metadata.

### Detection Logic

1. **Jujutsu Detection** (Primary Check)
   - Check for `.jj` directory or `.jj` file (workspace)
   - Jujutsu stores a `.jj` file in the workspace root that points to the actual `.jj` store location
   - If `.jj` exists, this is a Jujutsu workspace

2. **Git Detection** (Secondary Check)
   - Check for `.git` directory (regular repository)
   - Check for `.git` file (worktree pointer)
   - Git worktrees use a `.git` file containing `gitdir: <path>` pointing to the actual `.git` directory

3. **Worktree Classification**
   - For Jujutsu: A directory is a worktree if it contains a `.jj` file (pointer) rather than a `.jj` directory (main workspace)
   - For Git: A directory is a worktree if it contains a `.git` file (pointer) rather than a `.git` directory (main repository)

### Priority Rules

- Jujutsu takes precedence over Git when both indicators are present
- Worktree classification is determined by the pointer file vs directory distinction
- If neither `.git` nor `.jj` indicators are found, status is `none`

### Examples

- `~/src/compfuzor` → `git` (has `.git/` directory)
- `~/src/compfuzor-x` → `worktree-git` (has `.git` file with `gitdir:` pointer)
- `~/src/niri-mcp` → `jj` (has `.jj/` directory or `.jj` file in main workspace)
- `~/src/niri-mcp-x` → `worktree-jj` (has `.jj` file pointing to main workspace)

## Roadmap

### `reforrest` — replicate local state to remotes

- **[is-tree-reforrest-cli]** Replicate local git and jj states up to tracked remotes. Discovers trees, pushes ahead branches/bookmarks, reports results.
- **[is-tree-reforrest-starter]** Scaffold the reforrest crate with CLI subcommands (`push`, `status`, `ensure-remote`), depending on is-tree as a library.
- **[is-tree-reforrest-tangled]** Auto-create missing remotes via the `tn` (tangled.org) CLI before pushing.
- **[is-tree-reforrest-watchman]** Daemonized proactive sync — watches for new commits and pushes automatically.

### Staleness & reporting

- **[is-tree-duration-column]** Add an `age`/`duration` column showing relative time since last change (e.g. `2h`, `3d`, `6w`).
- **[is-tree-per-file-stats]** Recurse into trees and report per-file stats: modification time, age, size. Filter by age thresholds.
- **[is-tree-staleness-views]** Cohesive staleness dashboard — at-a-glance fresh/aging/stale indicators, summary stats, drill-down into stale trees.

### Interaction & workflows

- **[is-tree-sort-filter-date]** `--sort`, `--filter`, `--date` flags for controlling output ordering, type filtering, and date formatting.
- **[is-tree-scan-priority]** Stream recently-changed projects first during scans.
- **[is-tree-interactive-picker]** Built-in TUI picker with fuzzy search and multi-select for choosing trees.
- **[is-tree-fuzzel-pipeline]** Documented example pipeline: `is-tree --all --format "{directory}" | fuzzel --dmenu --multi`.
- **[is-tree-semantic-search]** Semantic search across trees via opencode run / ACP.
