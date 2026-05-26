# banco-proxy — Mosaik's HTTP gateway to @arkade-os/banco.
#
#   docker build -f services/banco-proxy/Dockerfile \
#                -t banco-proxy:dev services/banco-proxy
#
# Uses pnpm because @arkade-os/banco is installed from git and runs a build
# hook at install time. Bookworm base because banco's transitive dev deps
# (canvas, better-sqlite3) build native modules that need python3 +
# build-essential + libcairo when prebuild-install has no matching binary.
# See pnpm-workspace.yaml for the allowBuilds + blockExoticSubdeps flags.

ARG NODE_VERSION=22.12-bookworm-slim

# ─── builder ─────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION} AS builder

# Native-build deps for @arkade-os/sdk's transitive node-gyp packages.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates \
      git \
      python3 \
      build-essential \
      pkg-config \
      libcairo2-dev \
      libpango1.0-dev \
      libjpeg-dev \
      libgif-dev \
      librsvg2-dev \
 && rm -rf /var/lib/apt/lists/* \
 && npm install -g pnpm@10.25.0

WORKDIR /app

# Manifests + the postinstall patcher first so install caches across
# source-only edits. The patcher needs to exist before `pnpm install` runs
# its postinstall hook.
COPY package.json tsconfig.json pnpm-workspace.yaml ./
COPY scripts ./scripts
RUN pnpm install --frozen-lockfile=false

COPY src ./src
RUN pnpm run build

# Drop dev deps for the runtime layer. Keeps the prod node_modules slim.
RUN pnpm prune --prod

# ─── runtime ─────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION} AS runtime

# Cairo / pango runtime libs (no -dev variants) so the native modules load
# without the build toolchain. Trims ~300MB vs. keeping the builder image.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      libcairo2 \
      libpango-1.0-0 \
      libpangocairo-1.0-0 \
      libjpeg62-turbo \
      libgif7 \
      librsvg2-2 \
      tini \
      wget \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /app --shell /usr/sbin/nologin banco

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

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["node", "dist/index.js"]
