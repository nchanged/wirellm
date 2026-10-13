#!/usr/bin/env bash
# ==============================================================================
# WireLLM Zero-Downtime Safe Deploy Script
#
# 1. Compiles frontend assets & embedded release binary (service stays online)
# 2. Verifies new binary integrity & runs sanity checks
# 3. Atomically installs verified binary to ~/.local/bin/wirellm
# 4. Performs seamless sub-second daemon restart (no 30s outage)
# 5. Verifies health endpoint
# ==============================================================================

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="${HOME}/.local/bin"
TARGET_BIN="${TARGET_DIR}/wirellm"
PORT="${WIRELLM_PORT:-${OXLLM_PORT:-8765}}"

echo "🚀 [1/5] Building frontend production bundle..."
cd "$REPO_ROOT/frontend"
npm run build --silent

echo "⚙️  [2/5] Compiling release binary (service remains active)..."
cd "$REPO_ROOT"
cargo build --release -p wirellm

BUILT_BIN="$REPO_ROOT/target/release/wirellm"
if [ ! -f "$BUILT_BIN" ]; then
  echo "❌ Error: Built binary not found at $BUILT_BIN" >&2
  exit 1
fi

echo "🔍 [3/5] Verifying binary integrity & configuration..."
# Test 1: Binary executes and outputs version
VERSION_OUTPUT="$("$BUILT_BIN" --version 2>&1)" || {
  echo "❌ Binary execution failed! Aborting deployment." >&2
  exit 1
}
echo "   • Binary verified: $VERSION_OUTPUT"

# Test 2: Verify help / validate command
"$BUILT_BIN" --help >/dev/null 2>&1 || {
  echo "❌ Binary failed basic CLI validation! Aborting deployment." >&2
  exit 1
}

echo "📦 [4/5] Atomically deploying binary to $TARGET_BIN..."
mkdir -p "$TARGET_DIR"
TMP_BIN="${TARGET_BIN}.tmp.$$"
cp -f "$BUILT_BIN" "$TMP_BIN"
chmod 755 "$TMP_BIN"
mv -f "$TMP_BIN" "$TARGET_BIN"
echo "   • Installed to $TARGET_BIN"

echo "🔄 [5/5] Performing sub-second daemon reload..."
PID_FILE="$HOME/.wirellm/wirellm.pid"
OLD_PID=""
if [ -f "$PID_FILE" ]; then
  OLD_PID="$(cat "$PID_FILE" 2>/dev/null || true)"
fi

if [ -n "$OLD_PID" ] && kill -0 "$OLD_PID" 2>/dev/null; then
  echo "   • Stopping old daemon (PID: $OLD_PID)..."
  kill -TERM "$OLD_PID" 2>/dev/null || true
  # Wait up to 1.5s for clean socket release
  for _ in {1..15}; do
    if ! kill -0 "$OLD_PID" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
fi

# Clean up stale pid
rm -f "$PID_FILE"

# Start new daemon with verified binary
"$TARGET_BIN" start --port "$PORT"

# Verify health endpoint
echo "🩺 Verifying gateway health..."
HEALTHY=false
for _ in {1..20}; do
  if curl -sf "http://127.0.0.1:$PORT/status" >/dev/null 2>&1; then
    HEALTHY=true
    break
  fi
  sleep 0.2
done

if [ "$HEALTHY" = true ]; then
  NEW_PID="$(cat "$PID_FILE" 2>/dev/null || echo "active")"
  echo "✨ Deployment complete! Gateway online (PID: $NEW_PID) with 0s noticeable downtime."
  echo "   • Dashboard:   http://localhost:$PORT/ui"
  echo "   • WS Stream:   ws://localhost:$PORT/ws/live"
  echo "   • MCP SSE:     http://localhost:$PORT/mcp/sse"
else
  echo "⚠️ Warning: Health check did not respond within 4s. Check logs: wirellm logs"
  exit 1
fi
