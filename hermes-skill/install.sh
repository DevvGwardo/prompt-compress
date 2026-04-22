#!/usr/bin/env bash
# Install prompt-compress hermes skill
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SKILL_DIR="${HOME}/.hermes/skills/software-development/prompt-compress"

echo "Installing prompt-compress hermes skill..."
mkdir -p "$SKILL_DIR"
cp "$SCRIPT_DIR/SKILL.md" "$SKILL_DIR/SKILL.md"
echo "Installed to $SKILL_DIR/SKILL.md"
echo "Verify with: hermes skills list | grep prompt-compress"
