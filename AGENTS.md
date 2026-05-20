# Project Agent Instructions

## Graphify

Use the `graphify` skill when a task asks about project structure, architecture, code relationships, module boundaries, or impact analysis.

This repository has graphify enabled. Existing graph artifacts live in `graphify-out/`:

- `graphify-out/graph.html` for the interactive graph.
- `graphify-out/GRAPH_REPORT.md` for the audit report.
- `graphify-out/graph.json` for raw graph data.
- `graphify-out/manifest.json` and `graphify-out/cache/` for incremental regeneration.

When `graphify-out/` already exists, prefer consulting it before doing broad manual exploration. Regenerate the graph after substantial architecture, module, or documentation changes so the artifacts stay useful for future agents.
