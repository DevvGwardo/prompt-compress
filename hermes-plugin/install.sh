#!/bin/bash
# Install the prompt-compress Hermes plugin to ~/.hermes/plugins/

set -e

SRC_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST_DIR="${HOME}/.hermes/plugins/prompt-compress"

echo "Installing prompt-compress plugin from $SRC_DIR to $DEST_DIR..."

# Create destination directory
mkdir -p "$DEST_DIR"

# Copy plugin files
cp -f "$SRC_DIR/plugin.yaml" "$DEST_DIR/"
cp -f "$SRC_DIR/__init__.py" "$DEST_DIR/"

echo "✓ Plugin installed to $DEST_DIR"
echo ""
echo "To enable:"
echo "  1. Ensure prompt-compress SDK is installed: pip install prompt-compress"
echo "  2. Ensure the compress-api server is running (usually on localhost:3000)"
echo "  3. Restart Hermes agent"
echo ""
echo "Usage:"
echo "  /prompt-compress <text> [--aggressiveness N] [--model gpt-4]"
echo "  Or call the 'compress_prompt' tool from LLM reasoning."
echo ""
