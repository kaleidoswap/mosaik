#!/usr/bin/env bash
# Mosaik — local Elements regtest harness.
#
# Spins up a single-node Elements regtest for developing the Tessera covenant.
# Requires elementsd on PATH or via $ELEMENTSD_EXEC.
#   download: https://github.com/ElementsProject/elements/releases
#
# Usage:
#   ./scripts/regtest.sh up        start the node
#   ./scripts/regtest.sh mine [N]  mine N blocks (default 1)
#   ./scripts/regtest.sh cli ...   run elements-cli against the node
#   ./scripts/regtest.sh down      stop the node
set -euo pipefail

DATADIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.regtest"
ELEMENTSD="${ELEMENTSD_EXEC:-elementsd}"
ELEMENTS_CLI="${ELEMENTS_CLI_EXEC:-${ELEMENTSD%elementsd}elements-cli}"
RPCUSER="user"
RPCPASS="pass"

cli() {
    "$ELEMENTS_CLI" -datadir="$DATADIR" -rpcuser="$RPCUSER" -rpcpassword="$RPCPASS" "$@"
}

case "${1:-}" in
  up)
    mkdir -p "$DATADIR"
    cat > "$DATADIR/elements.conf" <<EOF
chain=elementsregtest
rpcuser=$RPCUSER
rpcpassword=$RPCPASS
validatepegin=0
EOF
    "$ELEMENTSD" -datadir="$DATADIR" -daemon
    echo "elementsd starting (datadir: $DATADIR)"
    sleep 2
    cli createwallet mosaik >/dev/null 2>&1 || cli loadwallet mosaik >/dev/null 2>&1 || true
    echo "regtest up — try: ./scripts/regtest.sh mine 101"
    ;;
  mine)
    ADDR=$(cli getnewaddress)
    cli generatetoaddress "${2:-1}" "$ADDR"
    ;;
  cli)
    shift
    cli "$@"
    ;;
  down)
    cli stop || true
    echo "elementsd stopped"
    ;;
  *)
    echo "usage: $0 {up|mine [N]|cli ...|down}" >&2
    exit 1
    ;;
esac
