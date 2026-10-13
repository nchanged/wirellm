#!/usr/bin/env bash
# ==============================================================================
# wirellm-service: Service control manager for WireLLM (Default Port 8765)
# Usage: wirellm-service [start|stop|restart|status|logs|foreground]
# ==============================================================================

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PID_FILE="$HOME/.wirellm/wirellm.pid"
LOG_FILE="$HOME/.wirellm/wirellm.log"
PORT="${WIRELLM_PORT:-${OXLLM_PORT:-8765}}"
CONFIG_FILE="${WIRELLM_CONFIG:-$HOME/.wirellm/config.yaml}"

mkdir -p "$HOME/.wirellm"

# Resolve wirellm binary
if [ -x "$HOME/.local/bin/wirellm" ]; then
  WIRELLM_BIN="$HOME/.local/bin/wirellm"
elif [ -x "$HOME/work/oxllm/target/release/wirellm" ]; then
  WIRELLM_BIN="$HOME/work/oxllm/target/release/wirellm"
elif command -v wirellm >/dev/null 2>&1; then
  WIRELLM_BIN="$(command -v wirellm)"
elif [ -x "$HOME/.local/bin/oxllm" ]; then
  WIRELLM_BIN="$HOME/.local/bin/oxllm"
else
  echo "❌ Error: wirellm binary not found. Build with 'cargo build --release' in ~/work/oxllm." >&2
  exit 1
fi

# Load user environment variables
if [ -f "$HOME/.wirellm/.env" ]; then
  set -a
  source "$HOME/.wirellm/.env" 2>/dev/null || true
  set +a
elif [ -f "$HOME/.oxllm/.env" ]; then
  set -a
  source "$HOME/.oxllm/.env" 2>/dev/null || true
  set +a
fi

is_running() {
  if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE" 2>/dev/null || true)
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
      return 0
    fi
  fi
  return 1
}

start() {
  if is_running; then
    PID=$(cat "$PID_FILE")
    echo "🟢 wirellm is already running (PID: $PID) on port $PORT."
    echo "   Dashboard:  http://localhost:$PORT/ui"
    echo "   MCP Server: http://localhost:$PORT/mcp/sse"
    return 0
  fi

  echo "🚀 Starting wirellm daemon on port $PORT..."
  "$WIRELLM_BIN" start --port "$PORT"
}

start_foreground() {
  if is_running; then
    echo "⚠️  wirellm is already running in background. Stopping it first..."
    stop
  fi
  echo "🚀 Starting wirellm in foreground on port $PORT (Ctrl+C to stop)..."
  exec "$WIRELLM_BIN" serve --port "$PORT" -v
}

stop() {
  if is_running; then
    PID=$(cat "$PID_FILE")
    echo "🛑 Stopping wirellm (PID: $PID)..."
    "$WIRELLM_BIN" stop
    rm -f "$PID_FILE"
    echo "✅ wirellm stopped."
  else
    echo "ℹ️  wirellm is not running."
    rm -f "$PID_FILE"
  fi
}

restart() {
  echo "🔄 Restarting wirellm service on port $PORT..."
  "$WIRELLM_BIN" restart --port "$PORT"
}

status() {
  if is_running; then
    PID=$(cat "$PID_FILE")
    echo "🟢 wirellm daemon is RUNNING (PID: $PID) on port $PORT"
    echo "   Dashboard:  http://localhost:$PORT/ui"
    echo "   MCP Server: http://localhost:$PORT/mcp/sse"
    echo ""
    "$WIRELLM_BIN" status --port "$PORT" 2>/dev/null || true
  else
    echo "🔴 wirellm daemon is NOT running on port $PORT."
  fi
}

logs() {
  "$WIRELLM_BIN" logs --follow
}

ACTION="${1:-status}"

case "$ACTION" in
  start)
    start
    ;;
  fg|foreground)
    start_foreground
    ;;
  stop)
    stop
    ;;
  restart)
    restart
    ;;
  status)
    status
    ;;
  logs)
    logs
    ;;
  deploy)
    exec "$SCRIPT_DIR/scripts/deploy.sh"
    ;;
  *)
    echo "Usage: $0 [start|stop|restart|status|logs|foreground|deploy]"
    exit 1
    ;;
esac
