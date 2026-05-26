# banco-proxy — Mosaik's HTTP gateway to @arkade-os/banco.
#
#   docker build -f services/banco-proxy/Dockerfile \
#                -t banco-proxy:dev services/banco-proxy
#
# Uses pnpm (not npm) because @arkade-os/banco is installed from git, runs a
# build hook at install time, and pulls a transitive git-dep
# (@arkade-os/sdk pinned to a personal fork). Both knobs live in
# pnpm-workspace.yaml — see the README's "Upstream caveat" for why.

ARG NODE_VERSION=22.12-alpine

# ─── builder ─────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION} AS builder

# pnpm via corepack (ships with Node 22).
RUN corepack enable && corepack prepare pnpm@10.25.0 --activate

WORKDIR /app

# Manifests first so install caches across source-only edits.
COPY package.json tsconfig.json pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile=false

COPY src ./src
RUN pnpm run build

# Drop dev deps for the runtime layer.
RUN pnpm prune --prod

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
