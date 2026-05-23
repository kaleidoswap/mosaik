#!/usr/bin/env bash
# Mosaik — Liquid testnet harness (local-node).
#
# Mirrors scripts/regtest.sh but runs a real Liquid testnet elementsd. The same
# wallet UI works on both chains; the only differences are which node it talks
# to and whether it can mine blocks (regtest: yes, testnet: no — blocks come
# from the network at ~1 min/block).
#
# Bootstrap (one-time per machine):
#   ./scripts/testnet.sh up      start the node, create wallets (idempotent;
#                                 re-run to check sync progress)
#   ./scripts/testnet.sh fund    hit the public faucet for the treasury, then
#                                 issue the demo test assets (USDT, EURx);
#                                 writes .testnet/assets.json
#
# Use:
#   ./scripts/testnet.sh serve   wallet UI on http://127.0.0.1:8081
#   ./scripts/testnet.sh cli ... pass commands to elements-cli
#   ./scripts/testnet.sh down    stop elementsd
#
# Requires: elementsd (Simplicity-capable), hal-simplicity, curl, python3.
# Defaults to ~/.simplex/bin/elementsd — override with ELEMENTSD_EXEC.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATADIR="$ROOT/.testnet"
ELEMENTSD="${ELEMENTSD_EXEC:-${HOME}/.simplex/bin/elementsd}"
ELEMENTS_CLI="${ELEMENTS_CLI_EXEC:-${ELEMENTSD%elementsd}elements-cli}"
RPCUSER="user"
RPCPASS="pass"
RPCPORT="7041"
UI_PORT="${MOSAIK_UI_PORT:-8081}"

ESPLORA="https://blockstream.info/liquidtestnet/api"
FAUCET="https://liquidtestnet.com/faucet"
ASSETS_FILE="$DATADIR/assets.json"

# ── output helpers ──────────────────────────────────────────────────────────
green()  { printf '\033[32m%s\033[0m\n' "$1"; }
yellow() { printf '\033[33m%s\033[0m\n' "$1"; }
red()    { printf '\033[31m%s\033[0m\n' "$1"; }

# ── elements-cli wrapper (same fallback to curl+RPC as scripts/regtest.sh) ──
# Honors -rpcwallet=NAME by either passing it through (elements-cli path) or
# by routing the JSON-RPC call to /wallet/NAME (curl-fallback path).
cli() {
    if [ -x "$ELEMENTS_CLI" ]; then
        "$ELEMENTS_CLI" -datadir="$DATADIR" -rpcport="$RPCPORT" \
            -rpcuser="$RPCUSER" -rpcpassword="$RPCPASS" "$@"
        return
    fi
    local wallet=""
    # Peel off any leading -rpcwallet=... flags before the method name.
    while [ "$#" -gt 0 ]; do
        case "$1" in
            -rpcwallet=*) wallet="${1#-rpcwallet=}"; shift ;;
            -*) shift ;;     # ignore other elements-cli flags in fallback mode
            *) break ;;
        esac
    done
    local method="$1"; shift
    local url="http://127.0.0.1:${RPCPORT}"
    [ -n "$wallet" ] && url="${url}/wallet/${wallet}"
    local params
    params=$(python3 - "$@" <<'PYEOF'
import json, sys
result = []
for a in sys.argv[1:]:
    if a in ('true', 'false', 'null'): result.append(json.loads(a))
    elif a[:1] in ('[', '{'): result.append(json.loads(a))
    else:
        try: result.append(float(a) if '.' in a else int(a))
        except ValueError: result.append(a)
print(json.dumps(result))
PYEOF
)
    local resp
    resp=$(curl -sf --user "${RPCUSER}:${RPCPASS}" \
        -H 'Content-Type: application/json' \
        --data "{\"jsonrpc\":\"1.0\",\"id\":\"m\",\"method\":\"${method}\",\"params\":${params}}" \
        "$url") \
        || { echo "RPC ${method}: connection refused or HTTP error" >&2; return 1; }
    echo "$resp" | python3 -c "
import json, sys
d = json.loads(sys.stdin.read())
if d.get('error'):
    e = d['error']
    print(json.dumps(e) if isinstance(e, dict) else str(e), file=sys.stderr)
    sys.exit(1)
r = d.get('result')
if isinstance(r, str): print(r)
elif r is not None: print(json.dumps(r))
"
}

wait_for_rpc() {
    for _ in $(seq 1 60); do
        cli getblockchaininfo >/dev/null 2>&1 && return 0
        sleep 0.5
    done
    red "elementsd RPC did not come up on port $RPCPORT" >&2; exit 1
}

# ── up: start the node + create wallets ─────────────────────────────────────
cmd_up() {
    if [ ! -x "$ELEMENTSD" ]; then
        red "elementsd not found at $ELEMENTSD" >&2
        red "Set ELEMENTSD_EXEC=/path/to/elementsd or install via smplx." >&2
        exit 1
    fi
    mkdir -p "$DATADIR"
    cat > "$DATADIR/elements.conf" <<EOF
chain=liquidtestnet
rpcuser=$RPCUSER
rpcpassword=$RPCPASS
txindex=1
[liquidtestnet]
rpcport=$RPCPORT
rpcbind=127.0.0.1
EOF
    if "$ELEMENTSD" -datadir="$DATADIR" -daemon 2>/dev/null; then
        yellow "elementsd starting (datadir: $DATADIR)"
    else
        yellow "elementsd already running, continuing wallet setup"
    fi
    wait_for_rpc
    cli createwallet mosaik       >/dev/null 2>&1 || cli loadwallet mosaik       >/dev/null 2>&1 || true
    cli createwallet mosaik-maker >/dev/null 2>&1 || cli loadwallet mosaik-maker >/dev/null 2>&1 || true
    cli createwallet mosaik-taker >/dev/null 2>&1 || cli loadwallet mosaik-taker >/dev/null 2>&1 || true

    local info blocks headers verprog
    info=$(cli getblockchaininfo)
    blocks=$(echo "$info"  | python3 -c "import json,sys; print(json.load(sys.stdin)['blocks'])")
    headers=$(echo "$info" | python3 -c "import json,sys; print(json.load(sys.stdin)['headers'])")
    verprog=$(echo "$info" | python3 -c "import json,sys; print(json.load(sys.stdin)['verificationprogress'])")
    green "Testnet node up — block $blocks / $headers (verification: $verprog)"
    if python3 -c "import sys; sys.exit(0 if float('$verprog') >= 0.9999 else 1)"; then
        green "Chain is synced. Next: ./scripts/testnet.sh fund"
    else
        yellow "Chain is still syncing. Re-run './scripts/testnet.sh up' to recheck."
        yellow "Wait for verification ≈ 1.0 before funding (first sync can take 15–30 min)."
    fi
}

cmd_down() {
    cli stop 2>/dev/null || true
    yellow "elementsd stopped"
}

# ── fund: faucet → treasury, then issue test assets ─────────────────────────
cmd_fund() {
    cli getblockchaininfo >/dev/null 2>&1 || { red "elementsd not running. Run ./scripts/testnet.sh up first."; exit 1; }

    # 1) Faucet → treasury
    yellow "==> Hitting the public faucet for the 'mosaik' treasury wallet"
    local addr resp txid
    addr=$(cli -rpcwallet=mosaik getnewaddress)
    # The faucet wants an unconfidential address.
    local unconf
    unconf=$(cli -rpcwallet=mosaik getaddressinfo "$addr" \
        | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('unconfidential', d.get('address','')))")
    green "Treasury address: $unconf"
    resp=$(curl -sS "${FAUCET}?address=${unconf}&action=lbtc" 2>/dev/null || true)
    txid=$(python3 - <<PYEOF
import json, re
t = """$resp"""
try:
    d = json.loads(t)
    print(d.get('txid') or d.get('tx_hash') or '')
except Exception:
    m = re.search(r'[0-9a-f]{64}', t)
    print(m.group(0) if m else '')
PYEOF
)
    if [ -z "$txid" ]; then
        red "Faucet didn't return a txid. Try in a browser:"
        red "  ${FAUCET}?address=${unconf}&action=lbtc"
        exit 1
    fi
    green "Faucet txid: $txid"

    # 2) Wait for the faucet tx to confirm in our wallet.
    yellow "==> Waiting for the faucet tx to confirm (~1 min per block)"
    local confirmations=0
    until [ "$confirmations" -gt 0 ]; do
        confirmations=$(cli -rpcwallet=mosaik gettransaction "$txid" 2>/dev/null \
            | python3 -c "import json,sys; print(json.load(sys.stdin).get('confirmations',0))" 2>/dev/null || echo 0)
        [ "$confirmations" -gt 0 ] && break
        printf "."
        sleep 10
    done
    echo
    green "Faucet tx confirmed ($confirmations conf)."

    # 3) Issue test assets if we haven't already.
    if [ -f "$ASSETS_FILE" ]; then
        green "Test assets already issued (see $ASSETS_FILE):"
        cat "$ASSETS_FILE"
        green "✓ Bootstrap complete. Next: ./scripts/testnet.sh serve"
        return 0
    fi
    yellow "==> Issuing test assets (USDT, EURx) from the treasury wallet"
    local usdt_resp eurx_resp usdt_id eurx_id
    usdt_resp=$(cli -rpcwallet=mosaik issueasset 100000 0)
    usdt_id=$(echo "$usdt_resp" | python3 -c "import json,sys; print(json.load(sys.stdin)['asset'])")
    eurx_resp=$(cli -rpcwallet=mosaik issueasset 100000 0)
    eurx_id=$(echo "$eurx_resp" | python3 -c "import json,sys; print(json.load(sys.stdin)['asset'])")
    green "USDT asset id: $usdt_id"
    green "EURx asset id: $eurx_id"

    # 4) Wait for the issuance txs to confirm (both likely land in the same block).
    yellow "==> Waiting for issuance txs to confirm"
    local height_then
    height_then=$(cli getblockcount)
    until [ "$(cli getblockcount)" -gt "$height_then" ]; do
        printf "."
        sleep 10
    done
    echo
    green "Issuances confirmed at block $(cli getblockcount)."

    # 5) Persist the asset map for the server.
    cat > "$ASSETS_FILE" <<EOF
{
  "USDT": "$usdt_id",
  "EURx": "$eurx_id"
}
EOF
    green "Wrote $ASSETS_FILE"
    green ""
    green "✓ Bootstrap complete. Next: ./scripts/testnet.sh serve"
}

cmd_serve() {
    if [ ! -f "$ASSETS_FILE" ]; then
        red "$ASSETS_FILE not found. Run ./scripts/testnet.sh fund first." >&2
        exit 1
    fi
    green "Starting Mosaik testnet UI on http://127.0.0.1:${UI_PORT}"
    MOSAIK_TESTNET_ASSETS_FILE="$ASSETS_FILE" \
        cargo run -q --manifest-path "$ROOT/Cargo.toml" -p mosaik-cli -- \
        serve-wallet --port "$UI_PORT" --network testnet
}

case "${1:-}" in
  up)    cmd_up ;;
  down)  cmd_down ;;
  fund)  cmd_fund ;;
  serve) cmd_serve ;;
  cli)   shift; cli "$@" ;;
  *)
    cat >&2 <<EOF
Mosaik — Liquid testnet.

One-time bootstrap (start from scratch):
  ./scripts/testnet.sh up      start elementsd on liquidtestnet, create wallets
                                (idempotent; re-run to check sync progress)
  ./scripts/testnet.sh fund    faucet → treasury, then issue USDT/EURx

Use:
  ./scripts/testnet.sh serve   wallet UI on http://127.0.0.1:${UI_PORT}
  ./scripts/testnet.sh cli ... pass commands to elements-cli
  ./scripts/testnet.sh down    stop the node
EOF
    exit 1
    ;;
esac
