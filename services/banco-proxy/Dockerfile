# banco-proxy — Mosaik's HTTP gateway to @arkade-os/banco.
#
#   docker build -f services/banco-proxy/Dockerfile \
#                -t banco-proxy:dev services/banco-proxy

ARG NODE_VERSION=22.12-alpine

# ─── builder ─────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION} AS builder

WORKDIR /app

# Manifests first so npm install caches across source-only edits.
COPY package.json tsconfig.json ./
RUN npm install --no-audit --no-fund

COPY src ./src
RUN npm run build

# Drop dev deps for the runtime layer.
RUN npm prune --omit=dev

# ─── runtime ─────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION} AS runtime

RUN addgroup -S banco && adduser -S -G banco -u 10001 -h /app banco

WORKDIR /app
USER banco

COPY --from=builder --chown=banco:banco /app/node_modules ./node_modules
COPY --from=builder --chown=banco:banco /app/dist         ./dist
COPY --from=builder --chown=banco:banco /app/package.json ./

ENV NODE_ENV=production \
    BANCO_PROXY_PORT=3001 \
    LOG_LEVEL=info

EXPOSE 3001

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=4 \
  CMD wget -q -O - http://127.0.0.1:3001/healthz | grep -q '"status":"ok"' || exit 1

CMD ["node", "dist/index.js"]
