#!/usr/bin/env bash
# Mosaik — Liquid testnet helpers.
#
# Mirrors the Blockstream simplicity-codespace approach: NO local elementsd.
# Public Liquid testnet faucet funds covenant addresses; public Esplora reads
# chain state and broadcasts; `hal-simplicity` builds PSETs.
#
# Two ways to interact:
#
#   1. Wallet UI (mirrors the regtest demo):
#        ./scripts/testnet.sh serve
#        open http://127.0.0.1:8081
#
#   2. CLI demo (one Tessera, end-to-end like the codespace's demo.sh):
#        ./scripts/testnet.sh make-offer [amount_b_sats [timeout [offer.json]]]
#        ./scripts/testnet.sh take-offer [offer.json]
#
# Deps: cargo  hal-simplicity  curl  python3
#
# Demo parameters (safe test-only keys; never use with real funds):
#   Maker private key : 0707...07  (secp256k1 test vector — [7]×32 bytes)
#   Maker pays to     : tex1qkkxzy9glfws4nc392an5w2kgjym7sxpshuwkjy
#                       (the Liquid testnet faucet return address)
#   Asset locked+paid : L-BTC (Liquid testnet policy asset)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HAL="${HAL_SIMPLICITY:-${HOME}/.cargo/bin/hal-simplicity}"
ESPLORA="https://blockstream.info/liquidtestnet/api"
FAUCET="https://liquidtestnet.com/faucet"
OFFER_DEFAULT="$ROOT/offer.json"
UI_PORT="${MOSAIK_UI_PORT:-8081}"

# BIP-341 NUMS unspendable internal key (same as the tessera crate and codespace).
INTERNAL_KEY="50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0"

# ── Demo maker constants (from `mosaik testnet-constants`) ──────────────────
MAKER_PK="989c0b76cb563971fdc9bef31ec06c3560f3249d6ee9e5d83c57625596e05f6f"
MAKER_ADDRESS="tex1qkkxzy9glfws4nc392an5w2kgjym7sxpshuwkjy"
MAKER_SPK_HASH="bcfbe70502021903755bb406a7c4681817be317affc7d1120de2041a9e06cfc5"
# L-BTC testnet asset id, internal byte order (reverse of RPC display order).
LBTC_ASSET="499a818545f6bae39fc03b637f2a4e1e64e590cac1bc3a6f6d71aa4443654c14"

# ── Helpers ─────────────────────────────────────────────────────────────────

green()  { printf '\033[32m%s\033[0m\n' "$1"; }
yellow() { printf '\033[33m%s\033[0m\n' "$1"; }
red()    { printf '\033[31m%s\033[0m\n' "$1"; }
pause()  { read -rp "Press Enter to continue…" _; echo; }

mosaik() {
    cargo run -q --manifest-path "$ROOT/Cargo.toml" -p mosaik-cli -- "$@"
}

# Wait for a tx to appear on Esplora; echoes the tx JSON.
wait_esplora() {
    local txid="$1"
    yellow "Waiting for $txid on Esplora (Liquid testnet ~1 min per block)…"
    for _ in $(seq 1 120); do
        local data
        data=$(curl -sS "$ESPLORA/tx/$txid" 2>/dev/null || true)
        if echo "$data" | python3 -c \
            "import json,sys; d=json.load(sys.stdin); print(d['vout'][0])" \
            >/dev/null 2>&1; then
            echo "$data"
            return 0
        fi
        printf "."
        sleep 1
    done
    echo ""
    red "Tx not found on Esplora after 120s — is the testnet reachable?" >&2
    exit 1
}

# ── serve ────────────────────────────────────────────────────────────────────

cmd_serve() {
    green "Starting Mosaik testnet UI on http://127.0.0.1:${UI_PORT}"
    green "(no local node — reads from Esplora, funds via the public faucet)"
    mosaik serve-wallet --port "$UI_PORT" --network testnet
}

# ── make-offer ───────────────────────────────────────────────────────────────

cmd_make_offer() {
    local amount_b_sats="${1:-99500}"    # leaves 500 sat fee headroom on a 100k faucet drop
    local timeout="${2:-500}"
    local offer_file="${3:-$OFFER_DEFAULT}"

    yellow "==> Step 1: Compile the Tessera covenant"
    local tessera_json
    tessera_json=$(mosaik tessera-json \
        --asset-b     "$LBTC_ASSET" \
        --amount-b    "$amount_b_sats" \
        --maker-pk    "$MAKER_PK" \
        --maker-spk-hash "$MAKER_SPK_HASH" \
        --timeout     "$timeout")
    local CMR PROGRAM
    CMR=$(echo "$tessera_json" | python3 -c "import json,sys; print(json.load(sys.stdin)['cmr'])")
    PROGRAM=$(echo "$tessera_json" | python3 -c "import json,sys; print(json.load(sys.stdin)['program'])")
    green "CMR: $CMR"
    pause

    yellow "==> Step 2: Derive the Liquid testnet covenant address"
    local info CONTRACT_ADDRESS
    info=$("$HAL" simplicity info --liquid "$PROGRAM")
    CONTRACT_ADDRESS=$(echo "$info" | python3 -c "import json,sys; print(json.load(sys.stdin)['liquid_testnet_address_unconf'])")
    green "Covenant address: $CONTRACT_ADDRESS"
    pause

    yellow "==> Step 3: Fund via the Liquid testnet faucet"
    yellow "  Requesting $amount_b_sats sats of L-BTC for $CONTRACT_ADDRESS…"
    local faucet_resp FAUCET_TXID
    faucet_resp=$(curl -sS "${FAUCET}?address=${CONTRACT_ADDRESS}&action=lbtc" 2>/dev/null || true)
    FAUCET_TXID=$(echo "$faucet_resp" | python3 -c "
import json,sys,re
t = sys.stdin.read()
try:
    d = json.loads(t)
    print(d.get('txid') or d.get('tx_hash') or '')
except Exception:
    m = re.search(r'[0-9a-f]{64}', t)
    print(m.group(0) if m else '')
" 2>/dev/null || true)
    if [ -z "$FAUCET_TXID" ]; then
        yellow "Faucet response: $faucet_resp"
        yellow "Faucet may be rate-limited. Fund manually:"
        yellow "  ${FAUCET}?address=${CONTRACT_ADDRESS}&action=lbtc"
        red "No txid returned — aborting." >&2; exit 1
    fi
    green "Faucet txid: $FAUCET_TXID"
    pause

    yellow "==> Step 4: Wait for the transaction to appear on Esplora"
    local tx_data SPK ASSET VALUE_SATS VALUE_BTC
    tx_data=$(wait_esplora "$FAUCET_TXID")
    echo ""
    SPK=$(echo "$tx_data"    | python3 -c "import json,sys; print(json.load(sys.stdin)['vout'][0]['scriptpubkey'])")
    ASSET=$(echo "$tx_data"  | python3 -c "import json,sys; print(json.load(sys.stdin)['vout'][0].get('asset',''))")
    VALUE_SATS=$(echo "$tx_data" | python3 -c "import json,sys; print(json.load(sys.stdin)['vout'][0].get('value',0))")
    VALUE_BTC=$(python3 -c "print('%.8f' % (int('$VALUE_SATS') / 1e8))")
    green "UTXO  scriptPubKey : $SPK"
    green "      asset        : $ASSET"
    green "      value        : $VALUE_BTC BTC ($VALUE_SATS sats)"
    pause

    yellow "==> Saving offer to $offer_file"
    python3 - <<EOF
import json
offer = {
    "faucet_txid":    "$FAUCET_TXID",
    "covenant_vout":  0,
    "cmr":            "$CMR",
    "program":        "$PROGRAM",
    "contract_address": "$CONTRACT_ADDRESS",
    "spk":            "$SPK",
    "asset":          "$ASSET",
    "value_sats":     $VALUE_SATS,
    "value_btc":      "$VALUE_BTC",
    "amount_b_sats":  $amount_b_sats,
    "maker_address":  "$MAKER_ADDRESS",
    "lbtc_asset":     "$LBTC_ASSET",
    "asset_b":        "$LBTC_ASSET",
    "maker_pk":       "$MAKER_PK",
    "maker_spk_hash": "$MAKER_SPK_HASH",
    "timeout":        $timeout,
}
with open("$offer_file", "w") as f:
    json.dump(offer, f, indent=2)
print(json.dumps(offer, indent=2))
EOF
    green ""
    green "✓ Offer saved to $offer_file"
    green "  Run: ./scripts/testnet.sh take-offer"
}

# ── take-offer ───────────────────────────────────────────────────────────────

cmd_take_offer() {
    local offer_file="${1:-$OFFER_DEFAULT}"
    if [ ! -f "$offer_file" ]; then
        red "Offer file not found: $offer_file" >&2
        red "Run ./scripts/testnet.sh make-offer first." >&2
        exit 1
    fi

    local offer
    offer=$(cat "$offer_file")
    read_offer() { echo "$offer" | python3 -c "import json,sys; print(json.load(sys.stdin)['$1'])"; }

    local FAUCET_TXID CMR PROGRAM SPK ASSET VALUE_BTC VALUE_SATS
    local AMOUNT_B_SATS ASSET_B MAKER_SPK_HASH_O MAKER_PK_O TIMEOUT
    FAUCET_TXID=$(read_offer faucet_txid)
    CMR=$(read_offer cmr)
    PROGRAM=$(read_offer program)
    SPK=$(read_offer spk)
    ASSET=$(read_offer asset)
    VALUE_BTC=$(read_offer value_btc)
    VALUE_SATS=$(read_offer value_sats)
    AMOUNT_B_SATS=$(read_offer amount_b_sats)
    ASSET_B=$(read_offer asset_b)
    MAKER_SPK_HASH_O=$(read_offer maker_spk_hash)
    MAKER_PK_O=$(read_offer maker_pk)
    TIMEOUT=$(read_offer timeout)

    local SETTLE_VOUT=0
    local FEE_SATS=500
    local FEE_BTC="0.00000500"

    local AMOUNT_B_BTC
    AMOUNT_B_BTC=$(python3 -c "print('%.8f' % (int('$AMOUNT_B_SATS') / 1e8))")

    local CHANGE_SATS CHANGE_BTC
    CHANGE_SATS=$(python3 -c "print(int('$VALUE_SATS') - int('$AMOUNT_B_SATS') - $FEE_SATS)")
    if python3 -c "import sys; sys.exit(0 if int('$CHANGE_SATS') >= 0 else 1)"; then
        CHANGE_BTC=$(python3 -c "print('%.8f' % (int('$CHANGE_SATS') / 1e8))")
    else
        red "Covenant UTXO ($VALUE_SATS sats) too small for amount_b ($AMOUNT_B_SATS) + fee ($FEE_SATS) sats." >&2
        red "Re-run make-offer with a smaller amount_b_sats." >&2
        exit 1
    fi

    green "Taking offer — settling the Tessera covenant."
    green "  Covenant UTXO : $FAUCET_TXID:0"
    green "  Covenant value: $VALUE_BTC BTC"
    green "  Pay maker     : $AMOUNT_B_BTC BTC → $MAKER_ADDRESS"
    [ "$CHANGE_SATS" -gt 0 ] && green "  Change (maker) : $CHANGE_BTC BTC"
    pause

    yellow "==> Step 1: Create unsigned PSET"
    local OUTPUTS_JSON
    if [ "$CHANGE_SATS" -gt 0 ]; then
        OUTPUTS_JSON="[{\"address\":\"$MAKER_ADDRESS\",\"asset\":\"$ASSET\",\"amount\":$AMOUNT_B_BTC},{\"address\":\"$MAKER_ADDRESS\",\"asset\":\"$ASSET\",\"amount\":$CHANGE_BTC},{\"fee\":$FEE_BTC}]"
    else
        OUTPUTS_JSON="[{\"address\":\"$MAKER_ADDRESS\",\"asset\":\"$ASSET\",\"amount\":$AMOUNT_B_BTC},{\"fee\":$FEE_BTC}]"
    fi
    local PSET1
    PSET1=$("$HAL" simplicity pset create --liquid \
        "[{\"txid\":\"$FAUCET_TXID\",\"vout\":0}]" \
        "$OUTPUTS_JSON" \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['pset'])")
    green "PSET1 created."
    pause

    yellow "==> Step 2: Attach covenant metadata to the input"
    local PSET2
    PSET2=$("$HAL" simplicity pset update-input --liquid \
        "$PSET1" 0 \
        -i "${SPK}:${ASSET}:${VALUE_BTC}" \
        -c "$CMR" \
        -p "$INTERNAL_KEY" \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['pset'])")
    green "PSET2 updated."
    pause

    yellow "==> Step 3: Build the SETTLE witness"
    local settle_json WIT_PROGRAM WIT_WITNESS
    settle_json=$(mosaik settle-json \
        --asset-b        "$ASSET_B" \
        --amount-b       "$AMOUNT_B_SATS" \
        --maker-pk       "$MAKER_PK_O" \
        --maker-spk-hash "$MAKER_SPK_HASH_O" \
        --timeout        "$TIMEOUT" \
        --settle-vout    "$SETTLE_VOUT")
    WIT_PROGRAM=$(echo "$settle_json" | python3 -c "import json,sys; print(json.load(sys.stdin)['program'])")
    WIT_WITNESS=$(echo "$settle_json" | python3 -c "import json,sys; print(json.load(sys.stdin)['witness'])")
    green "SETTLE witness ready."
    pause

    yellow "==> Step 4: Finalize PSET with program + witness"
    local PSET3
    PSET3=$("$HAL" simplicity pset finalize --liquid \
        "$PSET2" 0 "$WIT_PROGRAM" "$WIT_WITNESS" \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['pset'])")
    green "PSET3 finalized."
    pause

    yellow "==> Step 5: Extract raw transaction"
    local RAW_TX
    RAW_TX=$("$HAL" simplicity pset extract --liquid "$PSET3" \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['transaction_hex'])" 2>/dev/null \
        || "$HAL" simplicity pset extract --liquid "$PSET3" \
        | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('hex') or d.get('raw') or list(d.values())[0])")
    green "Raw tx: ${RAW_TX:0:40}…"
    pause

    yellow "==> Step 6: Broadcast to Liquid testnet"
    local TXID
    TXID=$(curl -sX POST "$ESPLORA/tx" -d "$RAW_TX")
    if echo "$TXID" | grep -qE '^[0-9a-f]{64}$'; then
        green "✓ Broadcast successful!"
        green "  Settlement txid: $TXID"
        green "  View at: https://blockstream.info/liquidtestnet/tx/$TXID?expand"
    else
        red "Broadcast response: $TXID" >&2
        exit 1
    fi
}

# ── dispatch ─────────────────────────────────────────────────────────────────

case "${1:-}" in
  serve)              cmd_serve ;;
  make-offer)         cmd_make_offer "${2:-}" "${3:-}" "${4:-}" ;;
  take-offer)         cmd_take_offer "${2:-}" ;;
  testnet-constants)  mosaik testnet-constants ;;
  *)
    cat >&2 <<EOF
Mosaik — Liquid testnet (no local node, codespace-style).

Wallet UI (mirrors the regtest demo):
  ./scripts/testnet.sh serve              UI on http://127.0.0.1:${UI_PORT}

Single-Tessera CLI demo (end-to-end like exercises/*/demo.sh in the codespace):
  ./scripts/testnet.sh make-offer [amount_b_sats [timeout [offer.json]]]
  ./scripts/testnet.sh take-offer [offer.json]

  ./scripts/testnet.sh testnet-constants  print the demo maker key constants

Examples:
  ./scripts/testnet.sh serve
  ./scripts/testnet.sh make-offer 99500 500 && ./scripts/testnet.sh take-offer
EOF
    exit 1
    ;;
esac
