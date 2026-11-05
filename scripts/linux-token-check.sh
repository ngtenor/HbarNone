#!/usr/bin/env bash
# Checks the HTS token code (Stages 2 to 4) on Linux. Meant to be run from the
# USB stick; the result log is written back to the same stick.
#   bash /run/media/nick/AEAC-A758/linux-token-check.sh
# Optional: set REPO=/path/to/HederaVault if the project is somewhere else.
set -uo pipefail

USB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_FILE="$USB_DIR/token-check-$(date +%Y%m%d-%H%M%S).log"
exec > >(tee "$LOG_FILE") 2>&1

REPO="${REPO:-}"
if [ -z "$REPO" ]; then
  for candidate in "$HOME/HederaVault" "/home/HederaVault"; do
    if [ -d "$candidate/.git" ]; then REPO="$candidate"; break; fi
  done
fi
if [ -z "$REPO" ] || [ ! -d "$REPO/.git" ]; then
  echo "Could not find the HederaVault folder. Re-run with: REPO=/path/to/HederaVault bash $0" >&2
  exit 1
fi
cd "$REPO" || exit 1

FAILED=()
step() { printf '\n=== %s ===\n' "$1"; }
run() {
  local name="$1"; shift
  step "$name"
  if "$@"; then echo "PASS: $name"; else echo "FAIL: $name"; FAILED+=("$name"); fi
}

step "Where we are"
echo "Project: $REPO"
git status --short | head -n 20

step "Updating the code (git fetch, checkout, pull)"
BRANCH="${BRANCH:-feature/token-associate-hide}"
if ! git fetch origin; then
  echo "git fetch failed. Check the network, then re-run." >&2
  exit 1
fi
# npm install on this machine rewrites the lockfiles; set only those aside.
if ! git diff --quiet -- package-lock.json desktop-ui/package-lock.json; then
  echo "Setting aside local lockfile changes (git stash)."
  git stash push -m "token-check lockfiles $(date +%Y%m%d-%H%M%S)" -- package-lock.json desktop-ui/package-lock.json
fi
if ! git checkout "$BRANCH" || ! git pull --ff-only origin "$BRANCH"; then
  echo "Could not switch to $BRANCH. Commit or stash local changes, then re-run." >&2
  exit 1
fi
echo "Now at: $(git rev-parse --abbrev-ref HEAD) $(git rev-parse --short HEAD)"
if ! grep -q "send_testnet_token" crates/hedera-tx/src/submit.rs; then
  echo "This checkout does not contain the token-send code yet (send_testnet_token)." >&2
  echo "Push the latest changes from the Windows machine, or re-run with BRANCH=<branch name>." >&2
  exit 1
fi

run "Rust tests (hedera-tx, wallet-core, hedera-mirror)" \
  cargo test -p hedera-tx -p wallet-core -p hedera-mirror
run "Rust check (whole workspace, includes the Tauri app)" cargo check --workspace
run "Rust formatting" cargo fmt --all -- --check
run "Frontend tests" bash -c 'cd desktop-ui && npm install --no-audit --no-fund && npm run lint && npm test'

step "Summary"
if [ "${#FAILED[@]}" -eq 0 ]; then
  echo "ALL CHECKS PASSED"
else
  echo "FAILED: ${FAILED[*]}"
fi
echo "Log saved to: $LOG_FILE"
echo "Send that log file back (or paste the last 40 lines)."
