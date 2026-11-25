#!/bin/sh
# Interpolate ${MOSAIK_API_HOST}/${MOSAIK_API_PORT} into the nginx config at
# container start so compose can rewire the upstream without rebuilding.
set -eu

: "${MOSAIK_API_HOST:=mosaik-api}"
: "${MOSAIK_API_PORT:=8080}"
# Relay origins the page may open WebSockets to, space separated (CSP connect-src).
: "${MOSAIK_CONNECT_SRC:=wss://relay.kaleidoswap.com https://api.rgbmap.org}"
export MOSAIK_API_HOST MOSAIK_API_PORT MOSAIK_CONNECT_SRC

envsubst '${MOSAIK_API_HOST} ${MOSAIK_API_PORT}' \
  < /etc/nginx/conf.d/default.conf.template \
  > /etc/nginx/conf.d/default.conf

envsubst '${MOSAIK_CONNECT_SRC}' \
  < /etc/nginx/conf.d/security-headers.conf.template \
  > /etc/nginx/conf.d/security-headers.conf

exec nginx -g 'daemon off;'
