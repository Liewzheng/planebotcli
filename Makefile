.DEFAULT_GOAL := help

.PHONY: help install build run test test-v lint format check e2e clean

# ──────────────────────────────────────────────
# Setup
# ──────────────────────────────────────────────

install: ## Install the CLI locally (pbot + planebotcli into ~/.cargo/bin)
	@echo "install - Install the CLI locally"

	cargo install --path crates/planebotcli-cli --locked

build: ## Build the workspace (debug)
	@echo "build - Build the workspace"

	cargo build --workspace

# ──────────────────────────────────────────────
# Development
# ──────────────────────────────────────────────

run: ## Run the CLI (pass ARGS="wi ls -p Frontend")
	@echo "run - Run the CLI"

	cargo run --quiet --bin pbot -- $(ARGS)

# ──────────────────────────────────────────────
# Quality
# ──────────────────────────────────────────────

test: ## Run the workspace test suite
	@echo "test - Run the workspace tests"

	cargo test --workspace

test-v: ## Run the workspace tests, showing test output
	@echo "test-v - Run the workspace tests (verbose)"

	cargo test --workspace -- --nocapture

lint: ## Run clippy with warnings as errors
	@echo "lint - Run clippy with warnings as errors"

	cargo clippy --workspace --all-targets -- -D warnings

format: ## Format the workspace with rustfmt
	@echo "format - Format the workspace"

	cargo fmt --all

check: lint test ## Run lint + tests

# ──────────────────────────────────────────────
# End-to-end & clean
# ──────────────────────────────────────────────

e2e: ## Run the live smoke test (needs ~/.plane_api; pass ARGS="-p PROJECT")
	@echo "e2e - Run the live smoke test"

	scripts/e2e.sh $(ARGS)

clean: ## Remove build artifacts
	@echo "clean - Remove build artifacts"

	cargo clean

# ──────────────────────────────────────────────
# Help
# ──────────────────────────────────────────────

help: ## Show available commands
	@echo "Available commands:"
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'
