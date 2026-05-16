#!/usr/bin/env bash
# Mosaik — local Elements regtest harness.
#
# Spins up a single-node Elements regtest for developing the Tessera covenant,
# and seeds the wallet with spendable L-BTC.
# Requires elementsd on PATH or via $ELEMENTSD_EXEC.
#   download: https://github.com/ElementsProject/elements/releases
#
# Usage:
#   ./scripts/regtest.sh up        start the node + fund the wallet
#   ./scripts/regtest.sh mine [N]  mine N blocks (default 1)
#   ./scripts/regtest.sh fund      sweep genesis free-coins into the wallet
#   ./scripts/regtest.sh cli ...   run elements-cli against the node
#   ./scripts/regtest.sh down      stop the node
set -euo pipefail

DATADIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.regtest"
ELEMENTSD="${ELEMENTSD_EXEC:-elementsd}"
ELEMENTS_CLI="${ELEMENTS_CLI_EXEC:-${ELEMENTSD%elementsd}elements-cli}"
RPCUSER="user"
RPCPASS="pass"
RPCPORT="7040"

# elementsregtest has no block subsidy. With initialfreecoins pinned, the
# genesis block carries one OP_TRUE output holding all the L-BTC at a
# deterministic outpoint — `fund` sweeps it into the wallet.
FREECOINS_TXID="027f015923d266ffb8af1fadd09a3743dffebd199e8f1e45c96ca12e58992d5f"

cli() {
    "$ELEMENTS_CLI" -datadir="$DATADIR" -rpcport="$RPCPORT" \
        -rpcuser="$RPCUSER" -rpcpassword="$RPCPASS" "$@"
}

wait_for_rpc() {
    for _ in $(seq 1 30); do
        cli getblockchaininfo >/dev/null 2>&1 && return 0
        sleep 0.5
    done
    echo "elementsd RPC did not come up" >&2
    exit 1
}

fund_wallet() {
    # The genesis free-coins UTXO is an OP_TRUE output — anyone can spend it.
    # Sweep it once into a wallet-owned (unconfidential) address.
    local txout
    txout=$(cli gettxout "$FREECOINS_TXID" 0 2>/dev/null || true)
    if [ -z "$txout" ] || [ "$txout" = "null" ]; then
        echo "wallet already funded"
        return 0
    fi
    local addr unconf raw
    addr=$(cli getnewaddress)
    unconf=$(cli getaddressinfo "$addr" | grep -oE '"unconfidential": "[^"]+"' \
        | sed 's/.*: "//; s/"//')
    raw=$(cli createrawtransaction \
        "[{\"txid\":\"$FREECOINS_TXID\",\"vout\":0}]" \
        "[{\"$unconf\":20999999.999},{\"fee\":0.001}]")
    cli sendrawtransaction "$raw" >/dev/null
    cli generatetoaddress 1 "$(cli getnewaddress)" >/dev/null
    echo "wallet funded with ~21M L-BTC"
}

case "${1:-}" in
  up)
    mkdir -p "$DATADIR"
    cat > "$DATADIR/elements.conf" <<EOF
chain=elementsregtest
rpcuser=$RPCUSER
rpcpassword=$RPCPASS
validatepegin=0
# index every tx so the covenant code can fetch arbitrary previous txs.
txindex=1
# elementsregtest has no block subsidy — seed spendable L-BTC at genesis.
initialfreecoins=2100000000000000
[elementsregtest]
rpcport=$RPCPORT
rpcbind=127.0.0.1
EOF
    "$ELEMENTSD" -datadir="$DATADIR" -daemon
    echo "elementsd starting (datadir: $DATADIR)"
    wait_for_rpc
    cli createwallet mosaik >/dev/null 2>&1 \
        || cli loadwallet mosaik >/dev/null 2>&1 || true
    fund_wallet
    echo "regtest up — chain height $(cli getblockcount)"
    ;;
  mine)
    cli generatetoaddress "${2:-1}" "$(cli getnewaddress)"
    ;;
  fund)
    fund_wallet
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
    echo "usage: $0 {up|mine [N]|fund|cli ...|down}" >&2
    exit 1
    ;;
esac
