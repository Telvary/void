#!/usr/bin/env bash
# Build and install the release binary.
#
# Usage:
#   ./scripts/build-install.sh                    # run pre-flight checks, build+install, health check
#   ./scripts/build-install.sh --skip-checks      # skip pre-flight checks (fast path)
#   ./scripts/build-install.sh /custom/dir        # install to a custom directory
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

SKIP_CHECKS=0
POSITIONAL=()
for arg in "$@"; do
  case "$arg" in
    --skip-checks) SKIP_CHECKS=1 ;;
    *) POSITIONAL+=("$arg") ;;
  esac
done

INSTALL_DIR="${POSITIONAL[0]:-$HOME/bin}"
BIN_NAME="void"

if [ "$SKIP_CHECKS" -eq 0 ]; then
  echo "==> Running pre-flight checks (fmt/clippy/test)…"
  "$SCRIPT_DIR/check.sh"
else
  echo "==> Skipping pre-flight checks (--skip-checks)"
fi

echo "==> Building release binary…"
cargo build --release

SRC="target/release/$BIN_NAME"
DEST="$INSTALL_DIR/$BIN_NAME"

# Stop any running sync daemon before replacing the binary.
# A running process with a replaced executable can enter uninterruptible
# sleep on macOS when the kernel tries to page in code from the old inode.
if [ -f "$DEST" ]; then
  "$DEST" sync --stop 2>/dev/null && echo "==> Stopped running sync daemon" || true
fi

if [ ! -f "$SRC" ]; then
  echo "Error: release binary not found at $SRC" >&2
  exit 1
fi

# If void is also installed via Homebrew, its symlink in the brew prefix can
# shadow this local build (or conflict with it, if you install into that
# prefix), depending on PATH order. Stop its daemon and unlink it so the freshly
# built binary is the one that runs. The formula stays installed — restore the
# Homebrew version any time with `brew link void`.
if command -v brew >/dev/null 2>&1 && brew list --formula 2>/dev/null | grep -qx "$BIN_NAME"; then
  BREW_BIN="$(brew --prefix)/bin/$BIN_NAME"
  [ -x "$BREW_BIN" ] && "$BREW_BIN" sync --stop >/dev/null 2>&1 || true
  echo "==> Homebrew $BIN_NAME detected — unlinking so the local build takes precedence"
  brew unlink "$BIN_NAME" >/dev/null 2>&1 || true
  echo "    (restore it later with: brew link $BIN_NAME)"
fi

mkdir -p "$INSTALL_DIR"

# Use a temp file + atomic rename so zombie processes holding the old inode
# don't block new executions (macOS keeps the old inode alive for them).
TMP_DEST="$INSTALL_DIR/.$BIN_NAME.tmp.$$"
cp "$SRC" "$TMP_DEST"
chmod 755 "$TMP_DEST"

# macOS: strip quarantine / provenance attributes that block unsigned binaries
if [ "$(uname)" = "Darwin" ]; then
  xattr -cr "$TMP_DEST" 2>/dev/null || true
fi

mv -f "$TMP_DEST" "$DEST"

echo "==> Installed $BIN_NAME → $DEST"

echo "==> Running post-install health check…"
"$DEST" doctor --non-interactive

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
  SHELL_RC="$HOME/.bashrc"
  if [ -n "${ZSH_VERSION:-}" ] || [[ "${SHELL:-}" == */zsh ]]; then
    SHELL_RC="$HOME/.zshrc"
  fi
  echo ""
  echo "Warning: $INSTALL_DIR is not on your PATH. Add it with:"
  echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> $SHELL_RC && source $SHELL_RC"
fi
