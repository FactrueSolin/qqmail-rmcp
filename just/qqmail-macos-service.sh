#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY_NAME="qqmail-rmcp"
SERVICE_NAME="${QQMAIL_RMCP_SERVICE_NAME:-cn.actrue.qqmail-rmcp}"
INSTALL_DIR="${QQMAIL_RMCP_INSTALL_DIR:-$HOME/.local/share/qqmail-rmcp}"
LOG_DIR="${QQMAIL_RMCP_LOG_DIR:-$HOME/Library/Logs/qqmail-rmcp}"
PLIST_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$PLIST_DIR/$SERVICE_NAME.plist"
ENV_FILE="${QQMAIL_RMCP_ENV_FILE:-$ROOT_DIR/.env}"
DOMAIN="gui/$(id -u)/$SERVICE_NAME"

usage() {
    cat <<EOF
Usage: $0 <command>

Commands:
  build             Build the release binary with cargo
  deploy            Build, install, write plist, and load the LaunchAgent
  start             Load the LaunchAgent if needed, then start the service
  stop              Stop and unload the LaunchAgent
  restart           Restart the LaunchAgent service
  delete            Stop the service and remove installed files
  status            Print launchctl status for the service
  logs [lines|-f]   Show logs; use -f to follow
  plist             Print the generated LaunchAgent plist

Environment overrides:
  QQMAIL_RMCP_SERVICE_NAME   default: cn.actrue.qqmail-rmcp
  QQMAIL_RMCP_INSTALL_DIR    default: ~/.local/share/qqmail-rmcp
  QQMAIL_RMCP_LOG_DIR        default: ~/Library/Logs/qqmail-rmcp
  QQMAIL_RMCP_ENV_FILE       default: <repo>/.env
EOF
}

require_macos() {
    if [[ "$(uname -s)" != "Darwin" ]]; then
        echo "This command uses launchctl and must run on macOS." >&2
        exit 1
    fi
}

xml_escape() {
    local value="$1"
    value="${value//&/&amp;}"
    value="${value//</&lt;}"
    value="${value//>/&gt;}"
    value="${value//\"/&quot;}"
    value="${value//\'/&apos;}"
    printf '%s' "$value"
}

build() {
    cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"
}

install_files() {
    mkdir -p "$INSTALL_DIR" "$LOG_DIR" "$PLIST_DIR"
    cp "$ROOT_DIR/target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"

    if [[ -f "$ENV_FILE" ]]; then
        cp "$ENV_FILE" "$INSTALL_DIR/.env"
    else
        echo "No .env copied. Create one from .env.example or set QQMAIL_RMCP_ENV_FILE before deploy." >&2
    fi
}

write_plist() {
    mkdir -p "$LOG_DIR" "$PLIST_DIR"

    local binary_path="$INSTALL_DIR/$BINARY_NAME"
    local stdout_path="$LOG_DIR/$BINARY_NAME.out.log"
    local stderr_path="$LOG_DIR/$BINARY_NAME.err.log"

    cat > "$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$(xml_escape "$SERVICE_NAME")</string>
    <key>ProgramArguments</key>
    <array>
        <string>$(xml_escape "$binary_path")</string>
    </array>
    <key>WorkingDirectory</key>
    <string>$(xml_escape "$INSTALL_DIR")</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$(xml_escape "$stdout_path")</string>
    <key>StandardErrorPath</key>
    <string>$(xml_escape "$stderr_path")</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>$(xml_escape "${RUST_LOG:-info,qqmail_rmcp=debug}")</string>
    </dict>
</dict>
</plist>
EOF
}

is_loaded() {
    launchctl print "$DOMAIN" >/dev/null 2>&1
}

load_service() {
    require_macos
    if is_loaded; then
        return
    fi

    launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
    launchctl enable "$DOMAIN"
}

unload_service() {
    require_macos
    if is_loaded; then
        launchctl bootout "gui/$(id -u)" "$PLIST_PATH"
    fi
}

deploy() {
    require_macos
    build
    install_files
    write_plist
    unload_service || true
    load_service
    launchctl kickstart -k "$DOMAIN"
    echo "Deployed $SERVICE_NAME"
    echo "Binary: $INSTALL_DIR/$BINARY_NAME"
    echo "Plist:  $PLIST_PATH"
    echo "Logs:   $LOG_DIR"
}

start() {
    require_macos
    if [[ ! -f "$PLIST_PATH" ]]; then
        echo "Plist not found: $PLIST_PATH. Run 'just deploy' first." >&2
        exit 1
    fi
    load_service
    launchctl kickstart -k "$DOMAIN"
}

stop() {
    unload_service
}

restart() {
    require_macos
    if [[ ! -f "$PLIST_PATH" ]]; then
        echo "Plist not found: $PLIST_PATH. Run 'just deploy' first." >&2
        exit 1
    fi
    load_service
    launchctl kickstart -k "$DOMAIN"
}

delete_service() {
    require_macos
    unload_service || true
    rm -f "$PLIST_PATH"

    if [[ -z "$INSTALL_DIR" || "$INSTALL_DIR" == "/" || "$INSTALL_DIR" == "$HOME" ]]; then
        echo "Refusing to remove unsafe install directory: $INSTALL_DIR" >&2
        exit 1
    fi

    rm -rf "$INSTALL_DIR"
    echo "Deleted $SERVICE_NAME service files. Logs remain in $LOG_DIR."
}

status() {
    require_macos
    if is_loaded; then
        launchctl print "$DOMAIN"
    else
        echo "$SERVICE_NAME is not loaded."
        [[ -f "$PLIST_PATH" ]] && echo "Plist exists: $PLIST_PATH"
    fi
}

logs() {
    require_macos
    mkdir -p "$LOG_DIR"
    local stdout_path="$LOG_DIR/$BINARY_NAME.out.log"
    local stderr_path="$LOG_DIR/$BINARY_NAME.err.log"
    touch "$stdout_path" "$stderr_path"

    if [[ "${1:-}" == "-f" || "${1:-}" == "--follow" ]]; then
        tail -f "$stdout_path" "$stderr_path"
        return
    fi

    local lines="${1:-200}"
    tail -n "$lines" "$stdout_path" "$stderr_path"
}

plist() {
    write_plist
    cat "$PLIST_PATH"
}

command="${1:-help}"
shift || true

case "$command" in
    build) build ;;
    deploy) deploy ;;
    start) start ;;
    stop) stop ;;
    restart) restart ;;
    delete|uninstall) delete_service ;;
    status) status ;;
    logs) logs "$@" ;;
    plist) plist ;;
    help|-h|--help) usage ;;
    *)
        usage >&2
        exit 1
        ;;
esac
