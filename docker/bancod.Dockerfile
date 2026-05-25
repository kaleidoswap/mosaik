# Build bancod (the Arkade taker bot) from upstream source.
#
# Upstream `arkade-os/bancod` ships a Dockerfile but does not yet publish
# images to ghcr. We pin a commit (BANCOD_REF) and build it ourselves.
# When/if an official image appears, swap this for a FROM and delete the
# build stage.
#
#   docker build -f docker/bancod.Dockerfile \
#     --build-arg BANCOD_REF=<commit-sha> \
#     -t bancod:dev .

ARG BANCOD_REF=main

# ─── builder ─────────────────────────────────────────────────────────────────
FROM golang:1.23-alpine AS builder

ARG BANCOD_REF
ARG BANCOD_REPO=https://github.com/arkade-os/bancod.git

RUN apk add --no-cache git build-base

WORKDIR /src
RUN git clone "${BANCOD_REPO}" . \
 && git checkout "${BANCOD_REF}" \
 && go mod download

# Build both binaries the upstream Makefile ships.
RUN CGO_ENABLED=0 GOOS=linux \
    go build -trimpath -ldflags="-s -w" -o /out/bancod ./cmd/bancod \
 && CGO_ENABLED=0 GOOS=linux \
    go build -trimpath -ldflags="-s -w" -o /out/banco ./cmd/banco

# ─── runtime ─────────────────────────────────────────────────────────────────
FROM alpine:3.20 AS runtime

RUN apk add --no-cache ca-certificates tini \
 && addgroup -S bancod \
 && adduser -S -G bancod -u 10001 -h /app bancod

WORKDIR /app
USER bancod

COPY --from=builder /out/bancod /usr/local/bin/bancod
COPY --from=builder /out/banco  /usr/local/bin/banco

EXPOSE 7080 7081

ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/bancod"]
