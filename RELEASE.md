# Releasing Mosaik

Mosaik ships as **four container images** (multi-arch) and a **`mosaik`
CLI binary** (linux-x86_64 + macOS-aarch64). Both flow from a single
git tag.

## What gets published

A push of a `v*` tag triggers `.github/workflows/release.yaml`, which:

1. Builds and pushes to `ghcr.io/kaleidoswap`:
   - `mosaik-api:<tag>` + `:latest`
   - `mosaik-webapp:<tag>` + `:latest`
   - `banco-proxy:<tag>` + `:latest`
   - `mosaik-relay-server:<tag>` + `:latest`
   All images are built for `linux/amd64` + `linux/arm64`.
2. Cross-compiles `mosaik` for `x86_64-unknown-linux-gnu` and
   `aarch64-apple-darwin`, tarballs each binary.
3. Creates a **draft** GitHub Release with the binaries attached and
   auto-generated changelog. A human reviews and publishes it.

CI (`.github/workflows/ci.yaml`) runs on every PR: fmt, clippy, cargo
test, TS typecheck + build for each service and the webapp, compose
validation, and `docker build` (no push) for each image.

## Versioning

Semver, with descriptive suffixes for milestone tags:

- `v0.1.x` — pre-platform (the Liquid hackathon binary). Kept on `main`
  history for the record.
- `v0.2.0-platform` — first release with the covenant-DEX platform
  topology in place (mosaik-api, mosaik-webapp, mosaik-relay-server,
  banco-proxy, mosaik-banco).
- `v0.3.x` — incremental improvements under that platform shape.
- `v0.x.0-banco-live` — when `services/banco-proxy` swaps its
  stubbed SDK calls for the real `@arkade-os/banco`.
- `v1.0.0-bridge` — Phase 8: cross-chain Liquid-Tessera ↔ Arkade-banco.

## Cutting a release

```bash
# 1. Make sure the branch is clean and CI is green on main.
git checkout main && git pull

# 2. Bump the workspace version in Cargo.toml if you intend to publish
#    Rust crates (not currently — they're path-deps inside the workspace).
#    For images-only releases, no Cargo bump is required.

# 3. Confirm one last local smoke.
make test          # cargo test
make build-images  # all four images compile

# 4. Tag & push.
git tag -a v0.2.0-platform -m "platform-release: covenant DEX abstraction"
git push origin v0.2.0-platform

# 5. Watch the release workflow.
gh run watch
```

## Image consumption (downstream)

```yaml
# A consuming compose.yaml
services:
  mosaik-api:
    image: ghcr.io/kaleidoswap/mosaik-api:v0.2.0-platform
  mosaik-webapp:
    image: ghcr.io/kaleidoswap/mosaik-webapp:v0.2.0-platform
    environment:
      MOSAIK_API_HOST: mosaik-api
      MOSAIK_API_PORT: "8080"
  banco-proxy:
    image: ghcr.io/kaleidoswap/banco-proxy:v0.2.0-platform
  mosaik-relay:
    image: ghcr.io/kaleidoswap/mosaik-relay-server:v0.2.0-platform
```

Pin to a specific tag in production; `:latest` is for tracking the
bleeding edge.

## CLI install (downstream)

```bash
gh release download v0.2.0-platform -p 'mosaik-*-linux-x86_64.tar.gz'
tar -xzf mosaik-v0.2.0-platform-linux-x86_64.tar.gz
sudo install -m 0755 mosaik /usr/local/bin/mosaik
mosaik --help
```

## Rollback

Releases are immutable: ghcr tags can be re-pulled and re-pushed only by
explicit overwrite. To roll a deployment back, redeploy the previous
`:vX.Y.Z` tag — no special workflow needed. The `latest` tag, by
convention, points at the most recent published release; if it ever
points at a regression, push the prior `vX.Y.Z` again with
`docker tag … :latest && docker push :latest` as a hot-fix.

## Secrets the workflows expect

Provided by GitHub by default:

- `GITHUB_TOKEN` — auto-issued per run, used for ghcr login (with the
  `packages: write` permission on the release job) and for the Release
  creation.

No third-party secrets are required.

## Local equivalents

| CI job | Local equivalent |
|---|---|
| `rust` | `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace` |
| `banco-proxy` | `cd services/banco-proxy && npm ci && npm run typecheck && npm run build` |
| `webapp` | `cd webapp && npm ci && npm run typecheck && npm run build` |
| `compose` | `docker compose --env-file docker/.versions.env -f docker/compose.yaml -f docker/compose.arkade-mutinynet.yaml config --quiet` |
| `docker` | `make build-images` |
