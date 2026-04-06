# pick-iter: fast candidate generation + interactive multi-select + deep inspection

A design for combining `is-tree-scan-priority` and `is-tree-fuzzel-pipeline` into a single fluid workflow: generate tree candidates quickly, interactively pick the ones you care about, then inspect those in depth.

## The problem

`is-tree --all` against a large `~/src` is slow. It scans every directory, resolves jj bookmarks, fetches commit dates, and only then prints results. For interactive use you want the opposite:

1. **Fast candidate list** — just directory names, streamed as discovered.
2. **Pick what matters** — multi-select from that list via fuzzel (or any picker).
3. **Deep inspect the few** — run full is-tree with all columns on just the selected paths.

The current tooling makes this clunky because is-tree doesn't separate "discovery" from "inspection."

## The dream pipeline

```bash
is-tree --scan | fuzzel --dmenu --multi | is-tree --format all
```

Three stages, three processes, one pipe. Let's unpack what each stage needs.

## Stage 1: fast candidate generation

```bash
is-tree --scan
```

`--scan` is a new mode. It does the minimum work to determine that a directory is a tree (git or jj) and emits its path. No commit dates, no bookmark resolution, no remote queries. Just:

- readdir the parent
- stat `.git` / `.jj` in each child
- print matching paths, one per line

This maps onto `is-tree-scan-priority` — but instead of "priority ordering," it's "stream immediately, skip everything expensive." The key insight: discovery is cheap, inspection is expensive. `--scan` only does discovery.

### Streaming

`--scan` should stream: print each path as it's found, don't buffer. This lets fuzzel start showing candidates before the scan finishes. A large `~/src` with hundreds of entries becomes interactive from the first few results.

### What about type info?

Emitting bare paths works, but we could optionally include the status:

```
jj  ~/src/is-tree
git ~/src/compfuzor
```

This lets fuzzel show context without slowing down discovery. The status is free — we already check `.jj`/`.git` to classify. A `--scan --format "{status} {directory}"` or `--scan -s` flag could control this. But for the pipeline use case, bare paths are the default since they pipe directly into is-tree positional args.

## Stage 2: interactive multi-select

```bash
fuzzel --dmenu --multi
```

This is existing fuzzel behavior. It reads lines from stdin, presents a searchable multi-select UI, and emits selected lines to stdout. No work for us.

For terminal-only environments, alternatives work identically:

```bash
is-tree --scan | fzf --multi | is-tree --format all
is-tree --scan | sk --multi | is-tree --format all
```

The pipeline is picker-agnostic.

## Stage 3: deep inspection

```bash
is-tree --format all
```

This is where the selected paths come in as positional arguments. is-tree already supports this:

```bash
is-tree ~/src/is-tree ~/src/compfuzor --format all
```

For a handful of selected paths, this is fast — full column resolution on 5-10 trees instead of 500.

## Does is-tree need changes?

### What already works

- **Positional args**: `is-tree path1 path2 path3` already works. The pipe `fuzzel | xargs is-tree` would work today if fuzzel emitted paths.
- **`--format all`**: already supported.
- **Discovery**: the core detection logic exists.

### What doesn't work yet

1. **No streaming/fast-scan mode.** `is-tree --all` does full inspection on every directory before printing anything. We need `--scan` (or similar) that emits paths immediately with minimal work.

2. **stdin as positional input.** The pipeline `fuzzel | is-tree` doesn't work because is-tree reads positional args from argv, not stdin. We'd need either:
   - `xargs` glue: `is-tree --scan | fuzzel --multi | xargs is-tree --format all` — works today but xargs breaks on paths with spaces unless you use `-d '\n'`.
   - A `--stdin` flag on is-tree to read paths from stdin, one per line. This is cleaner:
     ```bash
     is-tree --scan | fuzzel --multi | is-tree --stdin --format all
     ```
   - Or: detect piped stdin automatically (if stdin is a pipe and no positional args given, read paths from stdin). This is the most ergonomic — zero extra flags.

3. **Recurse-on-selected.** Once you've picked trees, you might want to go deeper — per-file stats inside those trees. That's `is-tree-per-file-stats`, which would compose naturally:
   ```bash
   is-tree --scan | fuzzel --multi | is-tree --stdin --files --format all
   ```

### Recommendation: `--stdin` (or auto-detect)

The smallest change with the biggest payoff:

- Add stdin reading when no positional args are given and stdin is a pipe (isatty check).
- Or an explicit `--stdin` flag if implicit behavior feels too magical.

This turns the three-stage pipeline into a first-class workflow without adding subcommands or restructuring is-tree.

## The full pipeline, revised

With `--scan` + stdin support:

```bash
# pick trees, inspect deeply
is-tree --scan | fuzzel --dmenu --multi | is-tree --format all

# pick trees, see per-file staleness
is-tree --scan | fuzzel --dmenu --multi | is-tree --stdin --files --older-than 30d

# pick trees, push them
is-tree --scan | fuzzel --dmenu --multi | reforrest push

# scoped to a subdirectory
is-tree --scan ~/src | fuzzel --dmenu --multi | is-tree --format all
```

## Design decisions to resolve

### `--scan` output format

Default: bare paths. Optionally include status with a flag. Rationale: bare paths are universally pipeable. Anything else breaks composability.

```bash
is-tree --scan              # one path per line
is-tree --scan --show-type  # "jj  ~/src/is-tree" — useful for visual scanning but not for piping back
```

### `--scan` vs `--all --format "{directory}"`

These overlap. `--all --format "{directory}"` already emits just paths. The difference is:

- `--all --format "{directory}"` does full inspection on every tree, then strips output to just the path column. Slow.
- `--scan` skips inspection entirely. Fast.

We could make `--all --format "{directory}"` fast by detecting that only `directory` is requested and short-circuiting inspection. But `--scan` is a clearer intent signal and doesn't require format parsing to optimize.

**Recommendation**: add `--scan` as a distinct fast-path. It's self-documenting and unambiguous.

### Per-file recursion after picking

The `--files` flag (from `is-tree-per-file-stats`) would make the final `is-tree` call recurse into each selected tree and report per-file stats. This is where the pipeline really shines: you pick 3 stale projects from hundreds, then immediately see which files in those projects are gathering dust.

```bash
is-tree --scan | fuzzel --dmenu --multi | is-tree --format all --files --sort age-
```

### Session state

Could we persist a "pick session" — e.g., a temporary file with selected paths — so you can re-inspect without re-picking?

```bash
is-tree --scan | fuzzel --multi | tee /tmp/picked | is-tree --format all
# later:
is-tree --stdin --format all --files < /tmp/picked
```

This works with plain Unix tooling. No special session mechanism needed.

## Summary of required is-tree changes

| Change | Purpose | Scope |
|--------|---------|-------|
| `--scan` flag | Fast path-only discovery, streamed output | New flag, reuses detection logic, skips inspection |
| Stdin path reading | Accept selected paths from pipe | Auto-detect (isatty) or explicit `--stdin` flag |
| (future) `--files` flag | Per-file stats on selected trees | Separate ticket (`is-tree-per-file-stats`) |

Two flags. That's it. The rest is Unix.
