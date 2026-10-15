#!/usr/bin/env bash
#
# Build gr0k from this checkout and install it as the managed ~/.gr0k/bin/gr0k.
#
# Mirrors install.sh's layout (versioned binary in ~/.gr0k/downloads/, atomic
# symlink in bin/) but sources the binary from a local `cargo install` of the
# composition-root crate `xai-grok-pager-bin` instead of a GitHub Release. Use
# it to run your own edits globally, or to upgrade a source checkout in place.
#
# Usage:
#   scripts/install-local.sh                 # build + install over ~/.gr0k/bin/gr0k
#   GR0K_SHARE_DIR=~/.gr0k-dev scripts/install-local.sh   # install elsewhere
#   scripts/install-local.sh --no-link       # build + stage in downloads/, don't swap the symlink
#
# Environment:
#   GR0K_SHARE_DIR             install root (default: ~/.gr0k; legacy HYPER_SHARE_DIR still honored)
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
GR0K_HOME="${GR0K_SHARE_DIR:-${HYPER_SHARE_DIR:-$HOME/.gr0k}}"
DO_LINK=1
[ "${1:-}" = "--no-link" ] && DO_LINK=0

err()  { printf 'error: %s\n' "$1" >&2; exit 1; }
info() { printf '\033[1;36m==>\033[0m %s\n' "$1"; }

[ -d "$CRATE_PATH" ] || err "run from the gr0k repo: $CRATE_PATH not found"

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
info "building gr0k ($PROFILE) — first build takes ~13 min"
CMAKE_POLICY_VERSION_MINIMUM=3.5 \
GROK_SHELL_BUNDLE_RG_PATH="$RG_PATH" \
    cargo install --locked --path "$CRATE_PATH" --profile "$PROFILE"

CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin/gr0k"
[ -x "$CARGO_BIN" ] || err "expected built binary at $CARGO_BIN"

# ── Stage as a versioned binary under ~/.gr0k/downloads/ ─────────────────────
VERSION="$("$CARGO_BIN" --version 2>/dev/null | awk '{print $2}')"
[ -n "$VERSION" ] || err "could not read version from built binary"
DOWNLOADS_DIR="$GR0K_HOME/downloads"; BIN_DIR="$GR0K_HOME/bin"
mkdir -p "$DOWNLOADS_DIR" "$BIN_DIR"
VERSIONED="gr0k-${VERSION}-${PLATFORM_OS}-${PLATFORM_ARCH}"
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

# Back up whatever bin/gr0k currently resolves to, then atomically repoint it.
if [ -e "$BIN_DIR/gr0k" ] || [ -L "$BIN_DIR/gr0k" ]; then
    PREV_TARGET="$(readlink "$BIN_DIR/gr0k" 2>/dev/null || true)"
    printf 'previous: %s\n' "${PREV_TARGET:-$BIN_DIR/gr0k}"
fi
TMP_LINK="$BIN_DIR/gr0k.install.$$"
ln -s "../downloads/$VERSIONED" "$TMP_LINK"
mv -f "$TMP_LINK" "$BIN_DIR/gr0k"

info "gr0k v$VERSION installed to $BIN_DIR/gr0k"
case ":$PATH:" in
    *":$BIN_DIR:"*) printf 'Run `gr0k` to get started.\n' ;;
    *) printf 'Note: %s is not on PATH; add it or run the binary by full path.\n' "$BIN_DIR" ;;
esac
