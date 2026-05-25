## Mosaik — top-level convenience targets.
##
##   make help           # list targets
##   make dev-arkade     # bring up the Arkade Mutinynet stack
##   make dev-liquid     # (placeholder for Phase 3.5) Liquid regtest stack
##   make dev-full       # both backends + base services
##   make down           # stop everything
##   make build-images   # build all first-party images
##   make test           # cargo test workspace
##
## All compose calls pin upstream versions via docker/.versions.env.

SHELL := /usr/bin/env bash

COMPOSE        := docker compose --env-file docker/.versions.env
COMPOSE_BASE   := -f docker/compose.yaml
COMPOSE_ARKADE := -f docker/compose.arkade-mutinynet.yaml

.PHONY: help
help:
	@awk 'BEGIN { FS = ":.*##"; printf "Targets:\n" } /^[a-zA-Z_-]+:.*##/ { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

## ─── dev stacks ─────────────────────────────────────────────────────────────

.PHONY: dev-arkade
dev-arkade: ## Bring up mosaik-api + Arkade Mutinynet stack
	$(COMPOSE) $(COMPOSE_BASE) $(COMPOSE_ARKADE) up -d
	@echo "Stack up. Tail logs with: make logs SERVICE=arkd"

.PHONY: dev-full
dev-full: ## Bring up every backend overlay we have
	$(COMPOSE) $(COMPOSE_BASE) $(COMPOSE_ARKADE) up -d

.PHONY: down
down: ## Stop and remove all Mosaik containers
	$(COMPOSE) $(COMPOSE_BASE) $(COMPOSE_ARKADE) down

.PHONY: nuke
nuke: ## Stop containers AND wipe their volumes (chain data, wallets — destructive)
	$(COMPOSE) $(COMPOSE_BASE) $(COMPOSE_ARKADE) down -v

.PHONY: ps
ps: ## Show running stack containers
	$(COMPOSE) $(COMPOSE_BASE) $(COMPOSE_ARKADE) ps

.PHONY: logs
logs: ## Follow logs for one service: make logs SERVICE=arkd
	$(COMPOSE) $(COMPOSE_BASE) $(COMPOSE_ARKADE) logs -f $(SERVICE)

## ─── image builds ───────────────────────────────────────────────────────────

.PHONY: build-images
build-images: build-mosaik-api build-mosaik-relay ## Build all first-party Mosaik images

.PHONY: build-mosaik-api
build-mosaik-api: ## Build mosaik-api:dev image
	docker build -f docker/mosaik-api.Dockerfile -t mosaik-api:dev .

.PHONY: build-mosaik-relay
build-mosaik-relay: ## Build mosaik-relay-server:dev image
	docker build -f services/mosaik-relay-server/Dockerfile -t mosaik-relay-server:dev services/mosaik-relay-server

.PHONY: build-bancod
build-bancod: ## Build bancod:dev image from upstream source
	docker build -f docker/bancod.Dockerfile -t bancod:dev .

## ─── workspace ──────────────────────────────────────────────────────────────

.PHONY: test
test: ## cargo test (all crates)
	cargo test

.PHONY: fmt
fmt: ## cargo fmt --check
	cargo fmt --all -- --check

.PHONY: clippy
clippy: ## cargo clippy with warnings as errors
	cargo clippy --all-targets -- -D warnings
