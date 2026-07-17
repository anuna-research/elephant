#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026 Anuna Research

set -e

# elephant release script.
# Usage: ./release.sh [version]
# Example: ./release.sh 0.1.0
#
# Bumps the version, runs the gate (test + clippy), commits, tags, and pushes.
# Pushing the tag triggers .woodpecker/release.yaml on Codeberg, which
# cross-compiles the four prebuilt binaries and publishes them (plus install.sh)
# to Cloudflare R2, served at https://files.anuna.io/elephant/.
#
# Requires the three sibling checkouts (path dependencies) for the local gate:
#   ../cbcl-rs  ../spindle-rust  ../did-crdt
#
# The Woodpecker pipeline builds against PINNED sibling commits (the *_REF
# values in .woodpecker/release.yaml). This script VERIFIES, before tagging,
# that the Cargo.lock it is about to ship both resolves and compiles under
# --locked against those exact pinned refs (see verify_lock_against_pins) — so a
# local sibling checkout that has drifted ahead of its pin can no longer produce
# a lock that passes here but fails in CI (as happened for v0.1.4). If the pins
# and the lock disagree, the release aborts and asks you to reconcile them
# deliberately: bump the pins to what you're shipping, or resolve the lock
# against the pins.

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Teardown for the verification worktrees. Uses the GLOBAL VERIFY_VDIR (not a
# function-local) so it still resolves when fired from an EXIT/INT trap after
# the creating function has already returned or been unwound by error()/Ctrl-C.
VERIFY_VDIR=""
_verify_teardown() {
  [[ -n "$VERIFY_VDIR" ]] || return 0
  git                    worktree remove --force "$VERIFY_VDIR/elephant"     2>/dev/null || true
  git -C ../cbcl-rs      worktree remove --force "$VERIFY_VDIR/cbcl-rs"      2>/dev/null || true
  git -C ../spindle-rust worktree remove --force "$VERIFY_VDIR/spindle-rust" 2>/dev/null || true
  git -C ../did-crdt     worktree remove --force "$VERIFY_VDIR/did-crdt"     2>/dev/null || true
  rm -rf "$VERIFY_VDIR"
  VERIFY_VDIR=""
}

# Reproduce CI's contract locally before we tag. The release pipeline builds
# with `cargo build --release --locked` against the PINNED sibling commits in
# .woodpecker/release.yaml — NOT against your local sibling checkouts. When a
# sibling checkout has drifted ahead of its pin (e.g. sitting on an unmerged
# feature branch), the Cargo.lock the gate just refreshed can carry dependency
# edges that don't exist at the pinned refs, and CI then dies with "cannot
# update the lock file because --locked was passed". We catch that here by
# resolving AND compiling the candidate lock against the pins, in throwaway
# worktrees so your actual sibling checkouts are never disturbed.
verify_lock_against_pins() {
  local wp=".woodpecker/release.yaml"
  [[ -f "$wp" ]] || error "Cannot find $wp to read the sibling pins."

  local cbcl_ref spindle_ref did_ref
  cbcl_ref=$(grep -oE 'CBCL_RS_REF=[0-9a-fA-F]{7,40}'      "$wp" | head -1 | cut -d= -f2)
  spindle_ref=$(grep -oE 'SPINDLE_RUST_REF=[0-9a-fA-F]{7,40}' "$wp" | head -1 | cut -d= -f2)
  did_ref=$(grep -oE 'DID_CRDT_REF=[0-9a-fA-F]{7,40}'      "$wp" | head -1 | cut -d= -f2)
  [[ -n "$cbcl_ref" && -n "$spindle_ref" && -n "$did_ref" ]] \
    || error "Could not parse CBCL_RS_REF / SPINDLE_RUST_REF / DID_CRDT_REF from $wp."

  # Make sure each pinned commit is present locally (fetch once if not).
  local pair sib ref
  for pair in "../cbcl-rs:$cbcl_ref" "../spindle-rust:$spindle_ref" "../did-crdt:$did_ref"; do
    sib="${pair%:*}"; ref="${pair##*:}"
    if ! git -C "$sib" cat-file -e "${ref}^{commit}" 2>/dev/null; then
      info "Fetching $sib to resolve pinned ref ${ref:0:12}..."
      git -C "$sib" fetch --quiet origin || true
      git -C "$sib" cat-file -e "${ref}^{commit}" 2>/dev/null \
        || error "Pinned ref $ref not found in $sib. Fix the pin in $wp or fetch the commit."
    fi
  done

  # Throwaway worktrees: siblings at their pinned refs, elephant at HEAD, laid
  # out as siblings so elephant's ../<name> path deps resolve to the pins.
  # Idempotent teardown on ANY exit (success, error() -> exit, or Ctrl-C) via
  # the global VERIFY_VDIR so the trap survives function unwinding.
  VERIFY_VDIR=$(mktemp -d)
  trap _verify_teardown EXIT INT TERM

  git                    worktree add --detach --force "$VERIFY_VDIR/elephant"     HEAD          >/dev/null 2>&1 || error "Could not create elephant verification worktree."
  git -C ../cbcl-rs      worktree add --detach --force "$VERIFY_VDIR/cbcl-rs"      "$cbcl_ref"    >/dev/null 2>&1 || error "worktree add cbcl-rs @ $cbcl_ref failed."
  git -C ../spindle-rust worktree add --detach --force "$VERIFY_VDIR/spindle-rust" "$spindle_ref" >/dev/null 2>&1 || error "worktree add spindle-rust @ $spindle_ref failed."
  git -C ../did-crdt     worktree add --detach --force "$VERIFY_VDIR/did-crdt"     "$did_ref"     >/dev/null 2>&1 || error "worktree add did-crdt @ $did_ref failed."

  # Overlay the candidate release inputs: the version-bumped Cargo.toml and the
  # Cargo.lock the local gate just refreshed.
  cp Cargo.toml Cargo.lock "$VERIFY_VDIR/elephant/"

  # Fast, exact reproduction of the CI failure: a lock inconsistent with the
  # pinned sources fails here instantly with "cannot update the lock file".
  if ! ( cd "$VERIFY_VDIR/elephant" && cargo metadata --locked --format-version 1 >/dev/null 2>&1 ); then
    error "Cargo.lock is INCONSISTENT with the pinned sibling refs in $wp.

Your local sibling checkouts have drifted from the release pins, so the lock the
gate just generated would fail CI's 'cargo build --release --locked'. Reconcile
deliberately, then re-run this script:
  - bump the *_REF pins in $wp to the sibling commits you intend to ship, OR
  - check the siblings out to the pinned refs and regenerate Cargo.lock.
Pinned refs:
  CBCL_RS_REF      = $cbcl_ref
  SPINDLE_RUST_REF = $spindle_ref
  DID_CRDT_REF     = $did_ref"
  fi

  # Stronger guarantee: elephant actually COMPILES against the pinned siblings
  # (catches code that needs an unmerged sibling API even when the lock is
  # coincidentally consistent). This is the slow step; it rebuilds in a fresh
  # target dir.
  info "Compiling against the pinned siblings (cargo check --locked)..."
  ( cd "$VERIFY_VDIR/elephant" && cargo check --locked --quiet ) \
    || error "elephant does not compile against the pinned sibling refs in $wp.
Bump the *_REF pins to include the sibling changes elephant now needs, or revert
the elephant code that depends on unmerged sibling APIs."

  # Success: tear down now and drop the trap so it doesn't fire again at the
  # script's normal exit.
  _verify_teardown
  trap - EXIT INT TERM
  info "Cargo.lock verified against the pinned siblings ✓"
}

VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
  CURRENT=$(grep '^version' Cargo.toml | head -1 | grep -o '"[^"]*"' | tr -d '"')
  echo "Current version: $CURRENT"
  read -p "Enter new version (without 'v' prefix): " VERSION
fi

[[ -z "$VERSION" ]] && error "Version is required"

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
  error "Invalid version format. Use semver: X.Y.Z or X.Y.Z-suffix"
fi

TAG="v$VERSION"

info "Preparing release $TAG"

if ! git diff --quiet || ! git diff --cached --quiet; then
  error "You have uncommitted changes. Commit or stash them first."
fi

if [[ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]]; then
  error "Release from main; you are on $(git rev-parse --abbrev-ref HEAD)."
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
  error "Tag $TAG already exists"
fi

for sib in ../cbcl-rs ../spindle-rust ../did-crdt; do
  [[ -d "$sib" ]] || error "Sibling $sib not found (required path dependency). Clone it next to elephant."
done

info "Updating version in Cargo.toml..."
if [[ "$(uname)" == "Darwin" ]]; then
  sed -i '' "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" Cargo.toml
else
  sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" Cargo.toml
fi

info "Running tests..."
cargo test --quiet

info "Running clippy..."
cargo clippy --all-targets --quiet -- -D warnings

# The local gate above ran against your local sibling checkouts. Now confirm the
# lock we're about to ship agrees with the PINNED siblings CI builds against, so
# a drifted local checkout can't produce a release that fails in CI (v0.1.4).
info "Verifying Cargo.lock against the pinned sibling refs..."
verify_lock_against_pins

info "Committing version bump..."
# `cargo test` above refreshes elephant's version in Cargo.lock.
git add Cargo.toml Cargo.lock
git commit -m "chore: release $TAG"

info "Creating tag $TAG..."
git tag -a "$TAG" -m "Release $VERSION"

info "Pushing to origin..."
git push origin main
git push origin "$TAG"

echo ""
info "Release $TAG published!"
echo ""
echo "Woodpecker release pipeline triggered (.woodpecker/release.yaml)."
echo "When it finishes, the release will be available at:"
echo "  https://files.anuna.io/elephant/            (latest)"
echo "  https://files.anuna.io/elephant/$TAG/"
echo ""
echo "Install:  curl https://files.anuna.io/elephant/install.sh | sh"
