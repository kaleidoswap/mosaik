# mosaik-relay-federation — sidecar that pulls events from peer relays
# and republishes them to the local mosaik-relay.
#
#   docker build -f services/mosaik-relay-server/federation/Dockerfile \
#                -t mosaik-relay-federation:dev services/mosaik-relay-server/federation

ARG NODE_VERSION=22.12-alpine

FROM node:${NODE_VERSION} AS builder

WORKDIR /app
COPY package.json tsconfig.json ./
RUN npm install --no-audit --no-fund
COPY src ./src
RUN npm run build
RUN npm prune --omit=dev

FROM node:${NODE_VERSION} AS runtime

RUN addgroup -S federation && adduser -S -G federation -u 10001 -h /app federation
WORKDIR /app
USER federation

COPY --from=builder --chown=federation:federation /app/node_modules ./node_modules
COPY --from=builder --chown=federation:federation /app/dist         ./dist
COPY --from=builder --chown=federation:federation /app/package.json ./

ENV NODE_ENV=production \
    LOG_LEVEL=info \
    LOCAL_RELAY=ws://mosaik-relay:7777

CMD ["node", "dist/index.js"]
