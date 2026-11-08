# mosaik-webapp — Vite SPA served by nginx.
#
#   docker build -f webapp/Dockerfile -t mosaik-webapp:dev webapp
#
# Two stages:
#   1. node:22-alpine builds the SPA (tsc + vite).
#   2. nginx:alpine serves the dist/ output and reverse-proxies /api +
#      /healthz to mosaik-api on the docker network. The upstream host
#      and port are interpolated from env at container start, so the
#      same image works in regtest, mutinynet and prod stacks.

ARG NODE_VERSION=22.12-alpine
ARG NGINX_VERSION=1.27-alpine

# ─── builder ─────────────────────────────────────────────────────────────────
FROM node:${NODE_VERSION} AS builder

WORKDIR /app

# Manifests first so npm install caches across source-only edits.
COPY package.json package-lock.json* ./
RUN npm install --no-audit --no-fund

COPY tsconfig.json vite.config.ts tailwind.config.js postcss.config.js index.html ./
COPY src ./src

RUN npm run build

# ─── runtime ─────────────────────────────────────────────────────────────────
FROM nginx:${NGINX_VERSION} AS runtime

# envsubst lives in gettext on alpine; tiny dep.
RUN apk add --no-cache gettext tini

# Strip the default config; install ours as a template so the entrypoint can
# fill in upstream host/port at container start.
RUN rm /etc/nginx/conf.d/default.conf
COPY nginx.conf /etc/nginx/conf.d/default.conf.template
COPY security-headers.conf /etc/nginx/conf.d/security-headers.conf.template
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# The built SPA.
COPY --from=builder /app/dist /usr/share/nginx/html

EXPOSE 80

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q -O - http://127.0.0.1/healthz-webapp | grep -q '"status":"ok"' || exit 1

ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/docker-entrypoint.sh"]
