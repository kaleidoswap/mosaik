# Mosaik Nostr relay — a curated nostr-rs-relay wrap.
#
# We don't fork the relay. We:
#   1. Pin a known-good `scsibug/nostr-rs-relay` image.
#   2. Bake our config.toml on top — kind-30050/30051/5 whitelist, retention,
#      connection limits.
#   3. Bind the relay's data volume to /var/lib/mosaik-relay.
#
# A future phase plugs in event-schema validation (the offer JSON in the
# content must match the OfferKind JSON Schema exported from mosaik-core).
# That hook lives outside the relay process so we don't have to maintain a
# fork — see the README in this directory.
#
#   docker build -f services/mosaik-relay-server/Dockerfile \
#                -t mosaik-relay-server:dev services/mosaik-relay-server

ARG RELAY_BASE=scsibug/nostr-rs-relay:0.9.0

FROM ${RELAY_BASE}

# nostr-rs-relay reads /usr/src/app/config.toml by default.
COPY config.toml /usr/src/app/config.toml

# Data lives here; mount a named volume in compose so settings survive restarts.
VOLUME ["/usr/src/app/db"]
EXPOSE 7777

# Inherits the upstream entrypoint / CMD. Restart policy is left to compose.
