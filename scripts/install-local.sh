#!/usr/bin/env bash
#
# Build hyper from this checkout and install it as the managed ~/.hyper/bin/hyper.
#
# Mirrors install.sh's layout (versioned binary in ~/.hyper/downloads/, atomic
# symlink in bin/) but sources the binary from a local `cargo install` of the
# composition-root crate `xai-grok-pager-bin` instead of a GitHub Release. Use
# it to run your own edits globally, or to upgrade a source checkout in place.
#
# Usage:
#   scripts/install-local.sh                 # build + install over ~/.hyper/bin/hyper
#   HYPER_SHARE_DIR=~/.hyper-dev scripts/install-local.sh   # install elsewhere
#   scripts/install-local.sh --no-link       # build + stage in downloads/, don't swap the symlink
#
# Environment:
#   HYPER_SHARE_DIR            install root (default: ~/.hyper)
#   GROK_SHELL_BUNDLE_RG_PATH  path to a real ripgrep binary to bundle
#                              (auto-detected from PATH / Homebrew if unset)
#   CARGO_PROFILE             cargo profile (default: release-dist)
#
# Run from anywhere; the script locates the repo root from its own location.
set -euo pipefail

# ── Locate repo root (this script lives in <root>/scripts/) ──────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

CRATE_PATH="crates/codegen/xai-grok-pager-bin"
PROFILE="${CARGO_PROFILE:-release-dist}"
HYPER_HOME="${HYPER_SHARE_DIR:-$HOME/.hyper}"
DO_LINK=1
[ "${1:-}" = "--no-link" ] && DO_LINK=0

err()  { printf 'error: %s\n' "$1" >&2; exit 1; }
info() { printf '\033[1;36m==>\033[0m %s\n' "$1"; }

[ -d "$CRATE_PATH" ] || err "run from the hyper repo: $CRATE_PATH not found"

# ── Platform naming (matches install.sh) ─────────────────────────────────────
OS="$(uname -s)"; ARCH="$(uname -m)"
case "$OS" in
    Darwin) PLATFORM_OS="macos" ;;
    Linux)  PLATFORM_OS="linux" ;;
    *)      err "unsupported OS: $OS" ;;
esac
case "$ARCH" in
    arm64|aarch64) PLATFORM_ARCH="aarch64" ;;
    x86_64|amd64)  PLATFORM_ARCH="x86_64"  ;;
    *)             err "unsupported arch: $ARCH" ;;
esac

# ── Dependency checks ────────────────────────────────────────────────────────
command -v cargo >/dev/null 2>&1 || err "cargo not found — install Rust (https://rustup.rs)"
command -v cmake >/dev/null 2>&1 || err "cmake not found — 'brew install cmake' (static Opus for /live voice)"

# A real ripgrep binary is needed by xai-grok-shell's build script. Note that in
# some shells `rg` is a function/alias, so resolve to an executable file.
resolve_rg() {
    if [ -n "${GROK_SHELL_BUNDLE_RG_PATH:-}" ] && [ -x "${GROK_SHELL_BUNDLE_RG_PATH}" ]; then
        printf '%s' "$GROK_SHELL_BUNDLE_RG_PATH"; return 0
    fi
    for c in /opt/homebrew/bin/rg /usr/local/bin/rg /usr/bin/rg "$(command -v rg 2>/dev/null || true)"; do
        [ -n "$c" ] && [ -x "$c" ] && [ ! -d "$c" ] && { printf '%s' "$c"; return 0; }
    done
    return 1
}
RG_PATH="$(resolve_rg || true)"
[ -n "$RG_PATH" ] || err "no ripgrep binary found — 'brew install ripgrep', or set GROK_SHELL_BUNDLE_RG_PATH"
info "using ripgrep: $RG_PATH"

# ── Build + install into ~/.cargo/bin (cargo's canonical location) ───────────
info "building hyper ($PROFILE) — first build takes ~13 min"
CMAKE_POLICY_VERSION_MINIMUM=3.5 \
GROK_SHELL_BUNDLE_RG_PATH="$RG_PATH" \
    cargo install --locked --path "$CRATE_PATH" --profile "$PROFILE"

CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin/hyper"
[ -x "$CARGO_BIN" ] || err "expected built binary at $CARGO_BIN"

# ── Stage as a versioned binary under ~/.hyper/downloads/ ─────────────────────
VERSION="$("$CARGO_BIN" --version 2>/dev/null | awk '{print $2}')"
[ -n "$VERSION" ] || err "could not read version from built binary"
DOWNLOADS_DIR="$HYPER_HOME/downloads"; BIN_DIR="$HYPER_HOME/bin"
mkdir -p "$DOWNLOADS_DIR" "$BIN_DIR"
VERSIONED="hyper-${VERSION}-${PLATFORM_OS}-${PLATFORM_ARCH}"
cp -f "$CARGO_BIN" "$DOWNLOADS_DIR/$VERSIONED"
chmod 0755 "$DOWNLOADS_DIR/$VERSIONED"

# cp breaks the arm64 ad-hoc code signature → macOS SIGKILLs the binary; re-sign.
if [ "$PLATFORM_OS" = "macos" ]; then
    xattr -cr "$DOWNLOADS_DIR/$VERSIONED" 2>/dev/null || true
    codesign -s - -f "$DOWNLOADS_DIR/$VERSIONED" >/dev/null 2>&1 || true
fi

# Smoke-test before swapping the active symlink (never break a working install).
"$DOWNLOADS_DIR/$VERSIONED" --version >/dev/null 2>&1 \
    || err "staged binary failed smoke test; existing install left untouched"

if [ "$DO_LINK" -eq 0 ]; then
    info "staged $DOWNLOADS_DIR/$VERSIONED (symlink not changed, --no-link)"
    exit 0
fi

# Back up whatever bin/hyper currently resolves to, then atomically repoint it.
if [ -e "$BIN_DIR/hyper" ] || [ -L "$BIN_DIR/hyper" ]; then
    PREV_TARGET="$(readlink "$BIN_DIR/hyper" 2>/dev/null || true)"
    printf 'previous: %s\n' "${PREV_TARGET:-$BIN_DIR/hyper}"
fi
TMP_LINK="$BIN_DIR/hyper.install.$$"
ln -s "../downloads/$VERSIONED" "$TMP_LINK"
mv -f "$TMP_LINK" "$BIN_DIR/hyper"

info "hyper v$VERSION installed to $BIN_DIR/hyper"
case ":$PATH:" in
    *":$BIN_DIR:"*) printf 'Run `hyper` to get started.\n' ;;
    *) printf 'Note: %s is not on PATH; add it or run the binary by full path.\n' "$BIN_DIR" ;;
esac
