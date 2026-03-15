# Testing Strategy

## Overview

is-tree needs tests for repo detection, CLI parsing, and output formatting. Tests use temp directories with scaffolded repos.

## Test Structure

```
tests/
├── unit/
│   ├── filter.rs        # parse_filters, matches_filters
│   ├── sort.rs          # parse_sort_specs, sort_results
│   ├── format.rs        # format parsing and output
│   └── status.rs        # get_status_string
├── detect/
│   ├── git.rs           # git detection, worktree detection
│   ├── jj.rs            # jj detection, worktree detection
│   └── ahead.rs         # ahead count calculation
└── integration/
    ├── cli.rs           # full CLI runs
    └── traversal.rs     # --all directory walking
```

## Test Fixtures

Temp repos created per-test using helpers:

```
tests/fixtures/           # static fixtures (committed)
├── expected/             # expected JSON outputs
│   └── simple-git.json
└── scripts/              # repo scaffolding scripts
    └── create-repo.sh

tests/temp/               # gitignored, created at runtime
```

## Test Helpers

```rust
// tests/common/mod.rs

/// Create a temp git repo with N commits
pub fn git_repo_with_commits(name: &str, count: usize) -> TempDir;

/// Create a temp jj repo with N commits  
pub fn jj_repo_with_commits(name: &str, count: usize) -> TempDir;

/// Create a git worktree from parent repo
pub fn git_worktree(parent: &Path, name: &str) -> PathBuf;

/// Create a jj worktree from parent repo
pub fn jj_worktree(parent: &Path, name: &str) -> PathBuf;

/// Set up remote tracking for ahead tests
pub fn with_remote_tracking(repo: &Path) -> (TempDir, TempDir); // (local, remote)
```

## Test Cases

### Unit: Detection

| Input | Expected |
|-------|----------|
| `.git/` exists | `RepoType::Git` |
| `.jj/` exists | `RepoType::Jujutsu` |
| `.git` is file | worktree-git |
| `.jj` symlink to parent | worktree-jj |
| neither exists | `RepoType::None` |

### Unit: Filters

| Filter | Repo | Matches |
|--------|------|---------|
| `git` | git | yes |
| `git` | jj | no |
| `git,jj` | git | yes |
| `worktree-` | worktree | no |
| `worktree-` | non-worktree | yes |

### Unit: Sort

| Spec | Behavior |
|------|----------|
| `status` | alphabetical status |
| `status-` | reverse alphabetical |
| `change-date-` | newest first |
| `directory+,status-` | directory asc, status desc |

### Integration: CLI

| Command | Verify |
|---------|--------|
| `is-tree .` | outputs status and directory |
| `is-tree --json .` | valid JSON |
| `is-tree --format '{ahead}' .` | only ahead column |
| `is-tree --all` | all subdirs |
| `is-tree --sort change-date- .` | sorted output |

### Integration: Ahead

| Setup | Expected |
|-------|----------|
| 3 local, 0 remote | ahead=3 |
| 3 local, 3 remote | ahead=0 |
| 0 local, 3 remote | TODO: ahead=-3 |
| no remote tracking | ahead=None |

## Running Tests

```bash
cargo nextest run           # all tests
cargo nextest run unit      # unit tests only
cargo nextest run --nocapture integration  # see output
```

## Dependencies

```toml
[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

## Out of Scope (Future)

- Benchmarking
- Fuzz testing
- Windows path handling
