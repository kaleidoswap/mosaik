#!/usr/bin/env bash
# Mosaik — seed the wallet UI with a live demo orderbook.
#
# Funds the user wallet and three independent market-maker wallets, then makes
# a spread of real covenant offers from those makers — both sides of several
# pairs — so http://127.0.0.1:8080 shows a populated, two-sided book with live
# on-chain data instead of an empty page.
#
# Prerequisites: the regtest node and the wallet UI server must be running —
#   ./scripts/regtest.sh up
#   cargo run -p mosaik-cli -- serve-wallet --port 8080
#
# Offers live in the server's memory, so re-seed after restarting the server.
#
# Usage:
#   ./scripts/demo-seed.sh [api-base-url]
set -euo pipefail

API="${1:-http://127.0.0.1:8080}"

post() {
    curl -s -X POST "$API/$1" -H 'Content-Type: application/json' -d "$2"
}

echo "Seeding the Mosaik demo orderbook at $API"
echo

echo "Funding wallets…"
for w in user mm-a mm-b mm-c; do
    post "api/fund" "{\"wallet\":\"$w\"}" >/dev/null
    echo "  $w funded with L-BTC and the test assets"
done
echo

echo "Making covenant offers from multiple makers…"
# Each offer funds one real covenant UTXO on the regtest.
#   amount_a / amount_b are raw units (1e8 per whole unit).
#   maker picks which market-maker wallet funds the covenant.
make_offer() {
    local label="$1" body="$2"
    local out
    out=$(post "api/make-offer" "$body")
    if echo "$out" | grep -q '"ok":true'; then
        echo "  ✓ $label"
    else
        echo "  ✗ $label — $out"
    fi
}

#          label                                  maker  lock  want  amount_a       amount_b
make_offer "Helix MM:  sell 0.010 L-BTC / 40 USDT"  '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":1000000,"amount_b":4000000000,"timeout":500}'
make_offer "Helix MM:  sell 0.025 L-BTC / 101 USDT" '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":2500000,"amount_b":10100000000,"timeout":500}'
make_offer "Aurora:    buy  L-BTC with 39 USDT"     '{"maker":"mm-b","lock":"USDT","want":"BTC","amount_a":3900000000,"amount_b":1000000,"timeout":500}'
make_offer "Aurora:    buy  L-BTC with 55 USDT"     '{"maker":"mm-b","lock":"USDT","want":"BTC","amount_a":5500000000,"amount_b":1430000,"timeout":500}'
make_offer "Tessera LP: sell 0.015 L-BTC / 55 EURx" '{"maker":"mm-c","lock":"BTC","want":"EURx","amount_a":1500000,"amount_b":5500000000,"timeout":500}'
make_offer "Tessera LP: buy L-BTC with 53 EURx"     '{"maker":"mm-c","lock":"EURx","want":"BTC","amount_a":5300000000,"amount_b":1500000,"timeout":500}'
make_offer "Helix MM:  sell 30 USDT / 27.6 EURx"    '{"maker":"mm-a","lock":"USDT","want":"EURx","amount_a":3000000000,"amount_b":2760000000,"timeout":500}'
make_offer "Aurora:    sell 40 EURx / 43 USDT"      '{"maker":"mm-b","lock":"EURx","want":"USDT","amount_a":4000000000,"amount_b":4300000000,"timeout":500}'

echo
echo "Done. Open $API — pick a pair from the top filter to browse the book."
