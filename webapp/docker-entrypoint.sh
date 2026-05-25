#!/bin/sh
# Interpolate ${MOSAIK_API_HOST}/${MOSAIK_API_PORT} into the nginx config at
# container start so compose can rewire the upstream without rebuilding.
set -eu

: "${MOSAIK_API_HOST:=mosaik-api}"
: "${MOSAIK_API_PORT:=8080}"
export MOSAIK_API_HOST MOSAIK_API_PORT

envsubst '${MOSAIK_API_HOST} ${MOSAIK_API_PORT}' \
  < /etc/nginx/conf.d/default.conf.template \
  > /etc/nginx/conf.d/default.conf

exec nginx -g 'daemon off;'
