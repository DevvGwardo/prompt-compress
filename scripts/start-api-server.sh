#!/usr/bin/env bash
# Start the compress-api server for development and testing.
#
# The server runs on http://localhost:3000 by default.
# Use Ctrl+C to stop it.
#
# Environment variables:
#   COMPRESS_API_HOST      Host to bind (default: 0.0.0.0)
#   PORT                   Port to listen on (default: 3000)
#   COMPRESS_API_KEY       Optional API key for authentication
#   COMPRESS_PROXY_*       Proxy mode settings (optional)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Starting compress-api server..."
echo "  Working directory: $REPO_DIR/crates/compress-api"
echo "  Server will be available at: http://localhost:${PORT:-3000}"
echo ""
echo "Press Ctrl+C to stop."
echo ""

cd "$REPO_DIR/crates/compress-api"

# Pass through relevant environment variables
export RUST_LOG="${COMPRESS_API_LOG:-info}"
cargo run
