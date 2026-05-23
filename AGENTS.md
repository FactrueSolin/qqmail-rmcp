# Project Agent Instructions

## Graphify

Use the `graphify` skill when a task asks about project structure, architecture, code relationships, module boundaries, or impact analysis.

This repository has graphify enabled. Existing graph artifacts live in `graphify-out/`:

- `graphify-out/graph.html` for the interactive graph.
- `graphify-out/GRAPH_REPORT.md` for the audit report.
- `graphify-out/graph.json` for raw graph data.
- `graphify-out/manifest.json` and `graphify-out/cache/` for incremental regeneration.

When `graphify-out/` already exists, prefer consulting it before doing broad manual exploration.

To view the current graph:

- Open `graphify-out/graph.html` in a browser for interactive navigation.
- Read `graphify-out/GRAPH_REPORT.md` for the summary, god nodes, communities, and suggested questions.
- Inspect `graphify-out/graph.json` when an agent or tool needs the raw graph data.

Regenerate the graph after substantial architecture, module, or documentation changes so the artifacts stay useful for future agents. If an LLM API key is not configured, reproduce the current no-cost code graph with:

```bash
python -m graphify update . --force
```

That command refreshes `graphify-out/graph.json`, `graphify-out/graph.html`, `graphify-out/GRAPH_REPORT.md`, `graphify-out/manifest.json`, and the AST cache using deterministic code extraction only.
