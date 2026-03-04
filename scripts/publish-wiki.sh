#!/usr/bin/env bash
set -euo pipefail

REPO_SLUG="DevvGwardo/prompt-compress"
WIKI_REMOTE="https://github.com/${REPO_SLUG}.wiki.git"
SOURCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/wiki"
WORK_DIR="${TMPDIR:-/tmp}/prompt-compress-wiki-publish"

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "Missing source wiki directory: $SOURCE_DIR" >&2
  exit 1
fi

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

if ! git clone "$WIKI_REMOTE" "$WORK_DIR" >/dev/null 2>&1; then
  cat >&2 <<MSG
Could not clone wiki remote: $WIKI_REMOTE

Likely cause: GitHub has not initialized the wiki git repository yet.
Open this once in your browser and create the first page:
  https://github.com/${REPO_SLUG}/wiki

Then rerun this script.
MSG
  exit 1
fi

cp "$SOURCE_DIR"/*.md "$WORK_DIR"/

cd "$WORK_DIR"
# Some environments have Git LFS lock verification enabled globally, which can
# break wiki pushes with:
# "Authentication required: You must have push access to verify locks".
# Disable lock verification for this wiki remote in the temp clone.
git config "lfs.${WIKI_REMOTE}/info/lfs.locksverify" false || true

if [[ -z "$(git status --porcelain)" ]]; then
  echo "No wiki changes to publish."
  exit 0
fi

git add *.md
git commit -m "Update wiki pages from repo source" >/dev/null
if ! GIT_LFS_SKIP_PUSH=1 git push origin master >/dev/null; then
  cat >&2 <<MSG
Wiki push failed.

If you see an authentication/lock verification error, run:
  gh auth login
  gh auth setup-git

Then retry:
  ./scripts/publish-wiki.sh
MSG
  exit 1
fi

echo "Wiki published successfully."
