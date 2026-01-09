# is-tree

> CLI utility to detect and classify Git and Jujutsu repositories

## Usage

```bash
# Test a specific directory
is-tree ~/src/compfuzor

# Test all directories in current path with --all/-a
is-tree --all

# Filter for specific types (comma-separated, - for NOT)
is-tree --all --filter git
is-tree --all --filter git,jj
is-tree --all --filter worktree-git,worktree-jj
is-tree --all --filter worktree-
is-tree --all --filter git-,jj

# Test specific directories by positional arguments
is-tree ~/src/compfuzor ~/src/niri-mcp

# Sort by any column (use + for ascending, - for descending)
is-tree --all --sort change-date-
is-tree --all --sort commit-date+
is-tree --all --sort workparent,directory+

# Multiple sorts (comma-separated)
is-tree --all --sort status-,change-date+

# Custom output format with interpolated columns
is-tree --all --format "{status} {directory} {commit-date} {change-date} {workparent}"

# Output as JSON
is-tree --all --json

# Custom date format
is-tree --all --date "%Y-%m-%d %H:%M:%S"
```

## Options

- `--all`, `-a` - Test all directories in current path
- `--filter <type>` - Filter by type(s). Multiple types comma-separated. Suffix `-` for NOT. Types: `git`, `jj`, `worktree`, `worktree-git`, `worktree-jj`
- `--sort <column>` - Sort by column(s). Multiple columns comma-separated. Suffix `+` for ascending, `-` for descending
- `--format <string>` - Custom output format with interpolated columns
- `--date <format>` - Date format string (default: ISO 8601)
- `--json` - Output results in JSON format
- Positional arguments - Test specific directories

## Output

Default format: `<status> <directory>`

### Columns
Available columns for custom format:
- `status` - Repository type (git, jj, worktree-git, worktree-jj, none)
- `directory` - Full directory path
- `workparent` - Directory name of the workparent (without path), for worktrees only
- `commit-date` - Most recent commit date
- `change-date` - Most recent file change date

### Status values
- `git` - Git repository (not Jujutsu, not a worktree)
- `jj` - Jujutsu repository (not a worktree)
- `worktree-git` - Git worktree
- `worktree-jj` - Jujutsu worktree
- `none` - Not a recognized repository

### JSON Output
When `--json` is specified, output is an array of objects with all available columns as fields.

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
