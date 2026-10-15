#!/usr/bin/env bash
#
# Hyper release helper — gr0kio/gr0kHyper.
#
# Cuts a release by tagging v<VERSION> and pushing the tag, which triggers
# .github/workflows/release.yml to cross-compile 5 targets and publish the
# GitHub Release. Optionally bumps the lockstep version across all files first.
#
# The lockstep version lives in 5 places that MUST stay in sync (CI compiles
# VERSION into x-grok-client-version; xAI rejects clients below 0.1.202):
#   VERSION
#   crates/codegen/xai-grok-version/Cargo.toml
#   crates/codegen/xai-grok-pager/Cargo.toml
#   crates/codegen/xai-grok-pager-bin/Cargo.toml
#   crates/codegen/xai-grok-shell/Cargo.toml
#
# Usage:
#   scripts/release.sh                 # release the current VERSION as-is
#   scripts/release.sh --bump patch    # bump 0.2.112 -> 0.2.113, then release
#   scripts/release.sh --bump minor    # bump 0.2.x  -> 0.3.0
#   scripts/release.sh --dry-run       # print every step, change nothing
#   scripts/release.sh --yes           # skip the confirmation prompt
#
# Before running with --bump, write your notes under "## [Unreleased]" in
# CHANGELOG.md — the bump moves them into the new version's section.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

BRANCH_EXPECTED="main"
MIN_VERSION="0.1.202"   # xAI rejects clients below this (HTTP 426)
VERSION_FILES=(
    "crates/codegen/xai-grok-version/Cargo.toml"
    "crates/codegen/xai-grok-pager/Cargo.toml"
    "crates/codegen/xai-grok-pager-bin/Cargo.toml"
    "crates/codegen/xai-grok-shell/Cargo.toml"
)

err()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
info() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
run()  { if [ "$DRY" -eq 1 ]; then printf '   [dry-run] %s\n' "$*"; else eval "$*"; fi; }

BUMP=""; DRY=0; ASSUME_YES=0
while [ $# -gt 0 ]; do
    case "$1" in
        --bump)     [ $# -ge 2 ] || err "--bump needs patch|minor|major"; BUMP="$2"; shift ;;
        --bump=*)   BUMP="${1#--bump=}" ;;
        --dry-run)  DRY=1 ;;
        --yes|-y)   ASSUME_YES=1 ;;
        -h|--help)  sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)          err "unknown argument: $1" ;;
    esac
    shift
done
[ -z "$BUMP" ] || case "$BUMP" in patch|minor|major) ;; *) err "invalid --bump '$BUMP' (patch|minor|major)";; esac

# ── Read + validate the current version ──────────────────────────────────────
command -v git >/dev/null 2>&1 || err "git not found"
CUR="$(tr -d '[:space:]' < VERSION)"
case "$CUR" in [0-9]*.[0-9]*.[0-9]*) ;; *) err "VERSION file has an invalid version: '$CUR'";; esac

# ── Compute the target version ───────────────────────────────────────────────
if [ -n "$BUMP" ]; then
    IFS=. read -r MA MI PA <<EOF
$CUR
EOF
    case "$BUMP" in
        patch) PA=$((PA + 1)) ;;
        minor) MI=$((MI + 1)); PA=0 ;;
        major) MA=$((MA + 1)); MI=0; PA=0 ;;
    esac
    NEW="$MA.$MI.$PA"
else
    NEW="$CUR"
fi
info "Current VERSION: $CUR    Release target: v$NEW${BUMP:+  (bump $BUMP)}"

# Guard against a version below the xAI floor.
lowest="$(printf '%s\n%s\n' "$MIN_VERSION" "$NEW" | sort -t. -k1,1n -k2,2n -k3,3n | head -n1)"
[ "$lowest" = "$MIN_VERSION" ] || err "target v$NEW is below the xAI minimum $MIN_VERSION (HTTP 426)"

# ── Preflight: branch, clean tree, sync with origin ──────────────────────────
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
[ "$BRANCH" = "$BRANCH_EXPECTED" ] || err "on branch '$BRANCH', expected '$BRANCH_EXPECTED'"
if [ -z "$BUMP" ]; then
    git diff --quiet && git diff --cached --quiet || err "working tree not clean (commit or stash first)"
fi
git fetch --quiet origin "$BRANCH_EXPECTED" 2>/dev/null || true
if git rev-parse "origin/$BRANCH_EXPECTED" >/dev/null 2>&1; then
    [ "$(git rev-parse HEAD)" = "$(git rev-parse "origin/$BRANCH_EXPECTED")" ] || {
        [ -n "$BUMP" ] || err "local $BRANCH_EXPECTED differs from origin/$BRANCH_EXPECTED (push/pull first)"
    }
fi

# ── Tag must not already exist ───────────────────────────────────────────────
TAG="v$NEW"
git rev-parse "$TAG" >/dev/null 2>&1 && err "tag $TAG already exists locally"
git ls-remote --exit-code --tags origin "$TAG" >/dev/null 2>&1 && err "tag $TAG already exists on origin"

# ── Apply the bump (version files + CHANGELOG), then commit ──────────────────
if [ -n "$BUMP" ]; then
    info "Bumping $CUR -> $NEW across ${#VERSION_FILES[@]} Cargo.toml + VERSION + CHANGELOG"
    if [ "$DRY" -eq 0 ]; then
        printf '%s\n' "$NEW" > VERSION
        for f in "${VERSION_FILES[@]}"; do
            [ -f "$f" ] || err "missing $f"
            perl -0pi -e "s/^version = \"\Q$CUR\E\"/version = \"$NEW\"/m" "$f"
            grep -q "^version = \"$NEW\"" "$f" || err "failed to update version in $f"
        done
        # Sync Cargo.lock to the bumped workspace versions. CI builds with
        # `--locked`, which HARD-FAILS if the lock still pins the old version.
        # --offline --workspace touches ONLY the four path crates (no registry
        # updates), so the release stays deterministic — just the version bump.
        command -v cargo >/dev/null 2>&1 || err "cargo not found (needed to sync Cargo.lock)"
        cargo update --workspace --offline >/dev/null 2>&1 \
            || cargo update --workspace >/dev/null 2>&1 \
            || err "failed to sync Cargo.lock to $NEW"
        grep -q "^version = \"$NEW\"" Cargo.lock || err "Cargo.lock did not pick up $NEW"
        # Move [Unreleased] notes into a dated [NEW] section.
        # -CSD keeps I/O in UTF-8 so the em-dash (and every other multibyte
        # char already in the file) is not double-encoded into mojibake.
        if grep -q "^## \[Unreleased\]" CHANGELOG.md; then
            perl -CSD -0pi -e "s/^## \[Unreleased\]\n/## [Unreleased]\n\n## [$NEW] \x{2014} $(date +%F)\n/m" CHANGELOG.md
        else
            err "CHANGELOG.md has no '## [Unreleased]' section to promote"
        fi
    else
        printf '   [dry-run] set VERSION and 4 Cargo.toml to %s\n' "$NEW"
        printf '   [dry-run] cargo update --workspace to sync Cargo.lock to %s\n' "$NEW"
        printf '   [dry-run] promote CHANGELOG [Unreleased] -> [%s] %s\n' "$NEW" "$(date +%F)"
    fi
    run "git add VERSION CHANGELOG.md Cargo.lock ${VERSION_FILES[*]}"
    run "git commit -q -m 'release: v$NEW'"
fi

# ── Consistency check: VERSION == every Cargo.toml ───────────────────────────
if [ "$DRY" -eq 0 ]; then
    v="$(tr -d '[:space:]' < VERSION)"
    [ "$v" = "$NEW" ] || err "VERSION ($v) != target ($NEW)"
    for f in "${VERSION_FILES[@]}"; do
        fv="$(sed -n 's/^version = "\(.*\)"/\1/p' "$f" | head -n1)"
        [ "$fv" = "$NEW" ] || err "version mismatch in $f: $fv != $NEW"
    done
    grep -q "^version = \"$NEW\"" Cargo.lock || err "Cargo.lock not synced to $NEW (run 'cargo update --workspace')"
    grep -q "^## \[$NEW\]" CHANGELOG.md || err "CHANGELOG.md has no '## [$NEW]' entry — add release notes first"
    info "Version consistency OK (VERSION + ${#VERSION_FILES[@]} crates + Cargo.lock + CHANGELOG)"
fi

# ── Confirm ──────────────────────────────────────────────────────────────────
printf '\n\033[1mAbout to release %s:\033[0m\n' "$TAG"
[ -n "$BUMP" ] && printf '  - push the release commit to origin/%s\n' "$BRANCH_EXPECTED"
printf '  - create annotated tag %s and push it (triggers release CI)\n\n' "$TAG"
if [ "$ASSUME_YES" -eq 0 ] && [ "$DRY" -eq 0 ]; then
    printf 'Proceed? [y/N] '; read -r ans
    case "$ans" in y|Y|yes) ;; *) err "aborted by user" ;; esac
fi

# ── Tag + push ───────────────────────────────────────────────────────────────
[ -n "$BUMP" ] && run "git push origin $BRANCH_EXPECTED"
run "git tag -a $TAG -m 'Hyper $TAG'"
run "git push origin $TAG"

info "Done. Release CI triggered for $TAG."
if command -v gh >/dev/null 2>&1 && [ "$DRY" -eq 0 ]; then
    printf 'Watch:   gh run watch --repo gr0kio/gr0kHyper $(gh run list --repo gr0kio/gr0kHyper --workflow=release.yml -L1 --json databaseId --jq ".[0].databaseId")\n'
    printf 'Release: https://github.com/gr0kio/gr0kHyper/releases/tag/%s\n' "$TAG"
fi
