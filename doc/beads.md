# Beads Integration

This document describes extractable metadata from beads-enabled projects.

## Beads Prefix

Each beads project has a prefix used for issue IDs (e.g., `is-tree` in `is-tree-e0b`).

**Extraction method:**
- Read `.beads/config.json` or similar beads configuration
- Parse the prefix from the database or config

**Use case:**
The `ahead` column could display beads ticket references using the project's prefix, linking unpushed commits to their associated work items.

## Future Extractable Subjects

- Open ticket count
- Current sprint/blockers
- Recent activity
- Ticket states for current branch commits
