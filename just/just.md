# qqmail-rmcp just scripts

These recipes build `qqmail-rmcp` and manage it as a macOS user LaunchAgent.

## Prerequisites

- macOS with `launchctl`
- Rust toolchain with `cargo`
- `just`
- A configured `config/qqmail.yaml` file copied from `config/qqmail.yaml.example`

## Common commands

Run commands from any directory inside the repository. `just` resolves the project root before executing recipes.

```bash
just build
```

Builds the release binary with Cargo.

```bash
just deploy
```

Builds the binary, installs it to `~/.local/share/qqmail-rmcp`, copies `config/qqmail.yaml` into `~/.local/share/qqmail-rmcp/config/qqmail.yaml` when present, copies `.env` when present for legacy fallback, writes `~/Library/LaunchAgents/cn.actrue.qqmail-rmcp.plist`, then loads and starts the LaunchAgent. When YAML is installed, the plist sets `QQMAIL_CONFIG` to the installed YAML path so the service does not depend on the repository checkout after deployment. When YAML is absent, the plist leaves `QQMAIL_CONFIG` unset so legacy `.env` fallback still works.

```bash
just redeploy
```

Stops and unloads the existing LaunchAgent, removes the old plist and install directory, then rebuilds, reinstalls, writes a fresh plist, loads, and starts the service again. Logs are kept under `~/Library/Logs/qqmail-rmcp`.

```bash
just restart
```

Restarts the loaded LaunchAgent service.

```bash
just stop
just start
```

Stops or starts the service without rebuilding.

```bash
just status
```

Prints `launchctl` status for the service.

```bash
just logs
just logs 500
just logs -f
```

Shows the last 200 log lines, a custom number of log lines, or follows the stdout and stderr logs.

```bash
just delete
```

Stops the LaunchAgent, removes the plist, and removes the installed binary/config directory. Logs are kept under `~/Library/Logs/qqmail-rmcp`.

## Paths and overrides

Defaults:

```bash
QQMAIL_RMCP_SERVICE_NAME=cn.actrue.qqmail-rmcp
QQMAIL_RMCP_INSTALL_DIR=$HOME/.local/share/qqmail-rmcp
QQMAIL_RMCP_LOG_DIR=$HOME/Library/Logs/qqmail-rmcp
QQMAIL_RMCP_CONFIG_FILE=<repo>/config/qqmail.yaml
QQMAIL_RMCP_ENV_FILE=<repo>/.env
```

Override these variables before running `just deploy` when a machine needs custom paths, a custom service label, or a YAML config stored outside the repository.

Example:

```bash
QQMAIL_RMCP_INSTALL_DIR=/opt/qqmail-rmcp just deploy
```

```bash
QQMAIL_RMCP_CONFIG_FILE=$HOME/secrets/qqmail.yaml just deploy
```
