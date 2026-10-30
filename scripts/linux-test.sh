#!/usr/bin/env bash
# Builds and checks HederaVault on Ubuntu/Debian. Run from anywhere:
#   bash scripts/linux-test.sh [--skip-apt] [--skip-bundle]
# Use only disposable wallets and recovery phrases when testing.
set -euo pipefail

SKIP_APT=0
SKIP_BUNDLE=0
for arg in "$@"; do
  case "$arg" in
    --skip-apt) SKIP_APT=1 ;;
    --skip-bundle) SKIP_BUNDLE=1 ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
LOG_FILE="$REPO_ROOT/linux-test-$(date +%Y%m%d-%H%M%S).log"
exec > >(tee "$LOG_FILE") 2>&1

step() { printf '\n=== %s ===\n' "$1"; }

step "System information"
uname -a
grep PRETTY_NAME /etc/os-release || true
df -h "$REPO_ROOT" | tail -n 1
git rev-parse --abbrev-ref HEAD
git rev-parse --short HEAD

if [ "$SKIP_APT" -eq 0 ]; then
  step "Installing Tauri 2 system packages"
  sudo apt-get update
  sudo apt-get install -y \
    build-essential curl wget file git pkg-config libssl-dev \
    libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
    libayatana-appindicator3-dev libxdo-dev
fi

step "Checking toolchains"
for tool in cargo rustc node npm; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Missing '$tool'. Install Rust with rustup (https://rustup.rs) and Node.js 20+ (nvm or NodeSource), then re-run." >&2
    exit 1
  fi
done
cargo --version
rustc --version
node --version
npm --version

step "Entropy source"
if [ -r /dev/urandom ]; then
  head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' | cut -c1-16
  echo "... /dev/urandom readable"
else
  echo "/dev/urandom is not readable" >&2
  exit 1
fi
cat /proc/sys/kernel/random/entropy_avail 2>/dev/null || true

step "Rust formatting"
cargo fmt --all -- --check

step "Rust tests"
cargo test --workspace

step "Rust check"
cargo check --workspace

step "Frontend dependencies"
npm install
(cd desktop-ui && npm install)

step "Frontend lint and build"
(cd desktop-ui && npm run lint && npm test && npm run build)

if [ "$SKIP_BUNDLE" -eq 0 ]; then
  step "Tauri production build"
  npm run tauri:build
  find target/release -maxdepth 3 \( -name 'hedera-vault-app' -o -name '*.deb' -o -name '*.AppImage' -o -name '*.rpm' \) -type f -print
else
  step "Tauri production build skipped"
fi

step "Existing app-data permissions (if the app has been launched)"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/org.hederavault.desktop"
if [ -d "$APP_DIR" ]; then
  stat -c '%A %U:%G %n' "$APP_DIR"
  [ -f "$APP_DIR/wallets.json" ] && stat -c '%A %U:%G %n' "$APP_DIR/wallets.json"
  DIR_MODE="$(stat -c '%a' "$APP_DIR")"
  if [ "$((8#$DIR_MODE & 8#077))" -ne 0 ]; then
    echo "WARNING: $APP_DIR is accessible by group/others."
  fi
  if [ -f "$APP_DIR/wallets.json" ]; then
    FILE_MODE="$(stat -c '%a' "$APP_DIR/wallets.json")"
    if [ "$((8#$FILE_MODE & 8#077))" -ne 0 ]; then
      echo "WARNING: wallets.json is accessible by group/others."
    fi
  fi
else
  echo "No app-data directory yet. Launch the app, create a disposable vault, then re-run with --skip-apt --skip-bundle."
fi

step "Next: manual checks"
echo "Launch: $REPO_ROOT/target/release/hedera-vault-app"
echo "Then work through the desktop verification list in docs/TODO.md using disposable wallets only."
echo "Log saved to: $LOG_FILE"
