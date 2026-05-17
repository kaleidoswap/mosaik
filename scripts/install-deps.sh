#!/usr/bin/env bash
# Mosaik — install the dependencies needed to run the local regtest.
#
# Sets up the three things `scripts/regtest.sh` and `mosaik serve-wallet` need:
#   1. the Rust toolchain (checked, not installed for you)
#   2. hal-simplicity   — assembles the covenant witness for a settlement
#   3. a Simplicity-capable elementsd — validates a covenant spend
#
# The Simplicity elementsd is NOT downloadable from a stable URL; it ships with
# the smplx / simplex toolchain (or the Blockstream simplicity-codespace). This
# script locates a build you already have, verifies it, and ad-hoc signs it on
# macOS. If none is found it prints where to get one and exits non-zero.
#
# Usage:
#   ./scripts/install-deps.sh
#
# On success it prints the `export ELEMENTSD_EXEC=...` line to use before
# `./scripts/regtest.sh up`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
green() { printf '\033[32m%s\033[0m\n' "$1"; }
yellow() { printf '\033[33m%s\033[0m\n' "$1"; }
red() { printf '\033[31m%s\033[0m\n' "$1"; }

# ---- 1. Rust toolchain ------------------------------------------------------

if ! command -v cargo >/dev/null 2>&1; then
    red "cargo not found. Install the Rust toolchain first:"
    echo "    curl https://sh.rustup.rs -sSf | sh"
    exit 1
fi
green "✓ Rust toolchain: $(cargo --version)"

# ---- 2. hal-simplicity ------------------------------------------------------

if command -v hal-simplicity >/dev/null 2>&1; then
    green "✓ hal-simplicity already installed: $(command -v hal-simplicity)"
else
    yellow "Installing hal-simplicity (cargo install)…"
    cargo install hal-simplicity
    green "✓ hal-simplicity installed"
fi

# ---- 3. Simplicity-capable elementsd ----------------------------------------

# Look in: $ELEMENTSD_EXEC, ./tools, ../tools, then PATH.
find_elementsd() {
    if [ -n "${ELEMENTSD_EXEC:-}" ] && [ -x "${ELEMENTSD_EXEC}" ]; then
        echo "${ELEMENTSD_EXEC}"; return 0
    fi
    for cand in "$ROOT/tools/elementsd" "$ROOT/../tools/elementsd"; do
        [ -x "$cand" ] && { echo "$(cd "$(dirname "$cand")" && pwd)/elementsd"; return 0; }
    done
    command -v elementsd 2>/dev/null && return 0
    return 1
}

if ELEMENTSD="$(find_elementsd)"; then
    green "✓ elementsd found: $ELEMENTSD"
else
    red "No elementsd found."
    echo
    echo "Mosaik needs a Simplicity-capable elementsd — stock Elements cannot"
    echo "validate a covenant spend. Get one from the smplx toolchain:"
    echo
    echo "    https://github.com/BlockstreamResearch/smplx"
    echo
    echo "or the Blockstream simplicity-codespace, then drop the binaries in"
    echo "    $ROOT/tools/"
    echo "and re-run this script (see README — Install)."
    exit 1
fi

BINDIR="$(dirname "$ELEMENTSD")"

# macOS kills unsigned downloaded binaries — ad-hoc sign them.
if [ "$(uname -s)" = "Darwin" ]; then
    for bin in elementsd elements-cli electrs simplex; do
        [ -x "$BINDIR/$bin" ] && codesign -s - -f "$BINDIR/$bin" 2>/dev/null \
            && green "✓ codesigned $bin" || true
    done
fi

# Verify it is an Elements build and actually runs.
if VERSION="$("$ELEMENTSD" -version 2>/dev/null | head -1)"; then
    green "✓ $VERSION"
    case "$VERSION" in
        *Elements*) : ;;
        *) yellow "  (warning: does not look like an Elements build)";;
    esac
else
    red "elementsd at $ELEMENTSD will not run — check the codesign step above."
    exit 1
fi

# ---- 4. build the workspace -------------------------------------------------

yellow "Building the Mosaik workspace…"
( cd "$ROOT" && cargo build )
green "✓ workspace built"

# ---- done -------------------------------------------------------------------

echo
green "All dependencies ready."
echo "Start the regtest and the wallet UI with:"
echo
echo "    export ELEMENTSD_EXEC=\"$ELEMENTSD\""
echo "    ./scripts/regtest.sh up"
echo "    cargo run -p mosaik-cli -- serve-wallet --port 8080"
