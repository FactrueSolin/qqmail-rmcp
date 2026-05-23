# Graph Report - qqmail-rmcp  (2026-05-23)

## Corpus Check
- 13 files · ~6,842 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 193 nodes · 289 edges · 13 communities (10 shown, 3 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `3cece13a`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 10|Community 10]]

## God Nodes (most connected - your core abstractions)
1. `resolve_account()` - 12 edges
2. `validate_required()` - 11 edges
3. `QQ Mail MCP Server - MVP Architecture` - 11 edges
4. `require_macos()` - 10 edges
5. `QqMailServer` - 10 edges
6. `tool_error()` - 9 edges
7. `Common commands` - 9 edges
8. `deploy()` - 8 edges
9. `parse_email_headers()` - 8 edges
10. `normalize_yaml_config()` - 7 edges

## Surprising Connections (you probably didn't know these)
- None detected - all connections are within the same source files.

## Communities (13 total, 3 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.07
Nodes (29): assert_uid_exists(), connect_imap(), delete_message(), DeleteMessageRequest, extract_html(), extract_text(), get_header_value(), get_message() (+21 more)

### Community 1 - "Community 1"
Cohesion: 0.11
Nodes (22): DeleteMessageParams, GetMessageParams, ListMailboxesParams, ListMessagesParams, MarkMessageParams, MoveMessageParams, QqMailServer, resolve_account() (+14 more)

### Community 2 - "Community 2"
Cohesion: 0.10
Nodes (28): AppConfig, MailAccountConfig, MailEndpointConfig, MailProvider, normalize_endpoint(), normalize_yaml_config(), parse_bind(), parse_yaml() (+20 more)

### Community 3 - "Community 3"
Cohesion: 0.11
Nodes (18): code:yaml (mcp:), code:block2 (src/), code:bash (cp .env.example .env), code:json ({), Configuration, Excluded (post-MVP), Included, Legacy Environment Variables (+10 more)

### Community 4 - "Community 4"
Cohesion: 0.25
Nodes (17): build(), delete_service(), deploy(), install_files(), is_loaded(), load_service(), logs(), plist() (+9 more)

### Community 5 - "Community 5"
Cohesion: 0.12
Nodes (15): code:bash (just build), code:bash (QQMAIL_RMCP_INSTALL_DIR=/opt/qqmail-rmcp just deploy), code:bash (QQMAIL_RMCP_CONFIG_FILE=$HOME/secrets/qqmail.yaml just deplo), code:bash (just deploy), code:bash (just redeploy), code:bash (just restart), code:bash (just stop), code:bash (just status) (+7 more)

### Community 6 - "Community 6"
Cohesion: 0.18
Nodes (10): code:bash (cp config/qqmail.yaml.example config/qqmail.yaml), code:yaml (mcp:), code:bash (cargo run), code:text (http://127.0.0.1:3000/mcp), code:json ({), code:json ({), mcp工具介绍, qqmail-rmcp (+2 more)

## Knowledge Gaps
- **54 isolated node(s):** `MailAccountConfig`, `MailProvider`, `MailEndpointConfig`, `RawRootConfig`, `RawMcpConfig` (+49 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **3 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What connects `MailAccountConfig`, `MailProvider`, `MailEndpointConfig` to the rest of the system?**
  _54 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.07188160676532769 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.10952380952380952 - nodes in this community are weakly interconnected._
- **Should `Community 2` be split into smaller, more focused modules?**
  _Cohesion score 0.10338680926916222 - nodes in this community are weakly interconnected._
- **Should `Community 3` be split into smaller, more focused modules?**
  _Cohesion score 0.10526315789473684 - nodes in this community are weakly interconnected._
- **Should `Community 5` be split into smaller, more focused modules?**
  _Cohesion score 0.125 - nodes in this community are weakly interconnected._