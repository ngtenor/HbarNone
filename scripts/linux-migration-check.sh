#!/usr/bin/env bash
# Guides the legacy-wallet migration check on Linux. Use disposable wallets only.
#   bash scripts/linux-migration-check.sh old      # build the pre-vault app; make 3 wallets with different passwords
#   bash scripts/linux-migration-check.sh new      # run the current app on that legacy file (run as often as needed)
#   bash scripts/linux-migration-check.sh restore  # put your own vault back and clean up
set -euo pipefail

REPO_ROOT="${HV_REPO:-$HOME/HederaVault}"
[ -d "$REPO_ROOT/.git" ] || { echo "Repo not found at $REPO_ROOT. Run: HV_REPO=/path/to/HederaVault bash $0 ${1:-old}"; exit 1; }
OLD_DIR="$HOME/hv-old"
OLD_COMMIT=18aed68
STATE="$HOME/hv-migration-test"
DEFAULT_DIR="$HOME/.local/share/org.hederavault.desktop"

found="$(find "$HOME" -name wallets.json -path '*hederavault*' -not -path "$REPO_ROOT/*" 2>/dev/null | head -1 || true)"
DATA_DIR="${found:+$(dirname "$found")}"
DATA_DIR="${DATA_DIR:-$DEFAULT_DIR}"
FILE="$DATA_DIR/wallets.json"
mkdir -p "$STATE"

sum() { [ -f "$FILE" ] && sha256sum "$FILE" | cut -d' ' -f1 || echo "(no file)"; }

case "${1:-}" in
  old)
    echo "Data folder: $DATA_DIR"
    pgrep -f hedera-vault-app >/dev/null && { echo "Close the HederaVault app first, then run this again."; exit 1; }
    if [ -f "$FILE" ] && [ ! -f "$STATE/my-vault.json" ]; then
      cp "$FILE" "$STATE/my-vault.json"
      echo "Saved your current vault to $STATE/my-vault.json"
    fi
    rm -f "$FILE"
    if [ ! -d "$OLD_DIR" ]; then
      cd "$REPO_ROOT"
      git worktree prune
      git worktree add "$OLD_DIR" "$OLD_COMMIT"
    fi
    cd "$OLD_DIR"
    npm ci
    echo
    echo ">>> In the old app: create 3 wallets, each with a DIFFERENT password, then close the app."
    npm run tauri:dev || true
    sum > "$STATE/legacy.sha"
    echo
    echo "Legacy file saved. Checksum: $(cat "$STATE/legacy.sha")"
    echo "Next: bash scripts/linux-migration-check.sh new"
    ;;
  new)
    [ -f "$STATE/legacy.sha" ] || { echo "Run the 'old' step first."; exit 1; }
    echo "Checksum of the legacy file now: $(sum)"
    echo
    echo ">>> In the app: enter two correct passwords and one WRONG one. It should refuse. Close the app."
    echo ">>> Run this step again later and enter all three correct passwords."
    cd "$REPO_ROOT"
    npm run tauri:dev || true
    echo
    now="$(sum)"
    if [ "$now" = "$(cat "$STATE/legacy.sha")" ]; then
      echo "RESULT: the file is UNCHANGED since the old build (expected after a wrong password)."
    else
      echo "RESULT: the file has CHANGED (expected only after a successful migration)."
    fi
    ;;
  restore)
    pgrep -f hedera-vault-app >/dev/null && { echo "Close the HederaVault app first."; exit 1; }
    if [ -f "$STATE/my-vault.json" ]; then
      mkdir -p "$DATA_DIR"
      cp "$STATE/my-vault.json" "$FILE"
      chmod 600 "$FILE"
      echo "Your own vault is back at $FILE"
    else
      echo "No saved vault found. Restore from your backup in the app instead."
    fi
    cd "$REPO_ROOT"
    git worktree remove --force "$OLD_DIR" 2>/dev/null || true
    git worktree prune
    rm -rf "$STATE/legacy.sha"
    echo "Cleaned up."
    ;;
  *)
    echo "Usage: bash scripts/linux-migration-check.sh old|new|restore"
    exit 2
    ;;
esac
