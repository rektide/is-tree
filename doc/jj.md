# Jujutsu (jj) Integration

This document describes the jj integration patterns, status parsing, and planned plugin functionality.

## `jj status` Output Format

The `jj status` command outputs working copy changes in a parseable format.

### Status Codes

| Code | Meaning | Description |
|------|---------|-------------|
| `A`  | Added   | New file not present in parent commit |
| `M`  | Modified| File changed from parent commit |
| `D`  | Deleted | File removed that existed in parent commit |
| `R`  | Renamed | File renamed (rare, usually shows as D+A) |
| `C`  | Copied  | File copied (rare) |

### Output Structure

```
Working copy changes:
A newfile.txt
M existing.txt
D removed.txt

Working copy  (@) : <change_id> <commit_id> (<description>)
Parent commit (@-): <change_id> <commit_id> <description>
```

When there are no changes:
```
The working copy has no changes.
Working copy  (@) : <change_id> <commit_id> (empty) (no description set)
Parent commit (@-): <change_id> <commit_id> <description>
```

### Parsing Strategy

1. Look for line `Working copy changes:` to identify the changes section
2. Parse subsequent lines starting with status codes (A, M, D, R, C)
3. Each status line format: `<CODE><SPACE><filename>`
4. End of changes section is marked by blank line or `Working copy` line

## Planned Plugin: Short Status with Change Counts

### Desired Metrics

| Metric | Description | Detection Method |
|--------|-------------|------------------|
| `jj-added` | Count of added files | Count lines starting with `A ` |
| `jj-modified` | Count of modified files | Count lines starting with `M ` |
| `jj-deleted` | Count of deleted files | Count lines starting with `D ` |
| `jj-status` | Net total changes | `jj-added + jj-modified + jj-deleted` |

### Implementation Approach

Run `jj status --ignore-working-copy` and parse output:

```bash
#!/bin/bash
# jj-short-status

output=$(jj status --ignore-working-copy 2>/dev/null)

# Check for "no changes" case
if echo "$output" | grep -q "The working copy has no changes"; then
    echo "jj-added=0"
    echo "jj-modified=0"
    echo "jj-deleted=0"
    echo "jj-status=0"
    exit 0
fi

# Parse the "Working copy changes:" section
changes=$(echo "$output" | sed -n '/^Working copy changes:/,/^$/p' | tail -n +2)

added=$(echo "$changes" | grep -c '^A ' || echo 0)
modified=$(echo "$changes" | grep -c '^M ' || echo 0)
deleted=$(echo "$changes" | grep -c '^D ' || echo 0)
total=$((added + modified + deleted))

echo "jj-added=$added"
echo "jj-modified=$modified"
echo "jj-deleted=$deleted"
echo "jj-status=$total"
```

### jj Alias Configuration

Add to `~/.config/jj/config.toml`:

```toml
[aliases]
# Simple count of mutable changes
cc = ["log", "--count", "-r", "immutable_heads().."]

# Short status via external script
sc = ["util", "exec", "--", "/path/to/jj-short-status"]
```

### Alternative: Inline Alias

For simple use cases, an inline bash script in the config:

```toml
[aliases]
wc = ["util", "exec", "--", "bash", "-c", """
jj status --ignore-working-copy 2>/dev/null | \
  sed -n '/^Working copy changes:/,/^$/p' | \
  tail -n +2 | wc -l | xargs -I{} echo "Changes: {}"
""", ""]
```

## Performance Considerations

- `--ignore-working-copy` avoids snapshotting overhead, useful for shell prompts
- For prompt integration, cache results with a timeout
- Consider using `JJ_WORKSPACE_ROOT` env var for multi-workspace awareness

## Related jj Commands

| Command | Purpose |
|---------|---------|
| `jj status` | Show working copy changes |
| `jj diff --summary` | Show diff with +/- file indicators |
| `jj diff --stat` | Show diff with histogram |
| `jj log --count -r <revset>` | Count commits in revset |
| `jj log -r "@" -T "diff.stats()"` | Get diff stats for working copy |

## Revset Reference for Counting

| Revset | Description |
|--------|-------------|
| `immutable_heads()..` | All mutable changes |
| `@` | Working copy commit |
| `@-` | Parent of working copy |
| `trunk()..@` | Changes ahead of trunk |
| `mine() & mutable()` | Your mutable changes |
