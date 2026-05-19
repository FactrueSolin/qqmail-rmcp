# QQ Mail MCP Server - MVP Architecture

## Overview

A local, single-account QQ Mail MCP server exposing email operations via Streamable HTTP. All operations use one QQ account configured via `.env`.

## MVP Scope

### Included
- Single QQ email account
- Token-protected `/mcp` endpoint
- Configurable bind address via `.env`
- Real email sending (no dry-run)
- HTML + plain text body support
- Full email body reading (no truncation)
- Delete, move, and mark (seen/flagged/answered) operations
- Priority use of `rmcp` crate for Streamable HTTP

### Excluded (post-MVP)
- Multi-account support
- Database / persistent storage
- Background sync
- Attachment binary download
- Web UI
- Public deployment
- REST API (MCP only)

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `QQMAIL_ADDRESS` | Yes | - | QQ email address |
| `QQMAIL_AUTH_CODE` | Yes | - | QQ mail authorization code (NOT password) |
| `QQMAIL_SMTP_HOST` | No | `smtp.qq.com` | SMTP server host |
| `QQMAIL_SMTP_PORT` | No | `465` | SMTP port (implicit TLS) |
| `QQMAIL_IMAP_HOST` | No | `imap.qq.com` | IMAP server host |
| `QQMAIL_IMAP_PORT` | No | `993` | IMAP port (implicit TLS) |
| `MCP_HTTP_BIND` | No | `127.0.0.1:3000` | HTTP bind address |
| `MCP_ACCESS_TOKEN` | Yes | - | Bearer token for /mcp access |

## MCP Interface

**Transport:** Streamable HTTP via `rmcp` crate
**Route:** `POST /mcp`
**Auth:** `Authorization: Bearer <MCP_ACCESS_TOKEN>`

### Tools

| Tool | Description | Side Effect |
|---|---|---|
| `send_email` | Send email via SMTP | Real send |
| `list_mailboxes` | List IMAP mailbox directories | Read-only |
| `list_messages` | List message summaries with pagination | Read-only |
| `get_message` | Get full email by UID | Read-only (mark_seen=false default) |
| `delete_message` | Delete message by UID | Destructive |
| `move_message` | Move message between mailboxes | State change |
| `mark_message` | Update message flags (seen/flagged/answered) | State change |

## Tech Stack

| Layer | Crate | Notes |
|---|---|---|
| MCP Protocol | `rmcp` 0.8 | Official Rust MCP SDK, Streamable HTTP |
| HTTP Framework | `axum` 0.8 | Token middleware, tower integration |
| Runtime | `tokio` 1 | Async runtime |
| SMTP | `lettre` 0.11 | Async SMTP with rustls TLS |
| IMAP | `imap` 1.0 | Blocking IMAP via `spawn_blocking` |
| MIME Parse | `mailparse` 0.16 | Header and body parsing |
| Config | `dotenvy` 0.15 | `.env` loading |
| Error | `thiserror` 2 | Derive macro |
| Logging | `tracing` + `tracing-subscriber` | Structured logging |

## Project Structure

```
src/
  main.rs          # Startup, logging, config, HTTP server with auth
  config.rs        # .env loading and validation
  error.rs         # Unified error types
  mcp.rs           # MCP server handler, tool definitions, routing
  mail/
    mod.rs         # Mail module re-exports
    smtp.rs        # SMTP send email via lettre
    imap.rs        # IMAP operations (list, get, delete, move, mark)
```

## Security

- `.env` credentials never logged or exposed
- MCP token required for all `/mcp` requests
- Unauthorized requests rejected with 401 before any tool execution
- Default bind to `127.0.0.1` (localhost only)
- SMTP/IMAP use implicit TLS (ports 465/993)

## Running

```bash
cp .env.example .env
# Edit .env with your QQ mail credentials
cargo run
# Server starts at http://127.0.0.1:3000/mcp
```

## MCP Client Config

```json
{
  "mcpServers": {
    "qqmail": {
      "url": "http://127.0.0.1:3000/mcp",
      "headers": {
        "Authorization": "Bearer <your-token>"
      }
    }
  }
}
```
