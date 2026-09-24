.PHONY: help build build-release test run run-latest clean

# Default target
.DEFAULT_GOAL := help

help: ## Show this help message
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z0-9_-]+:.*?## / {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build the project in debug mode
	cargo build

build-release: ## Build the project in release mode
	cargo build --release

test: ## Run tests
	cargo test

run: ## Run the CLI in debug mode (e.g., make run ARGS="list" or make run ARGS="--job 123")
	cargo run -- $(ARGS)

run-latest: build-release ## Run the CLI in release mode (e.g., make run-latest ARGS="--job 123")
	./target/release/dbt-ops $(ARGS)

fmt: ## Format the Rust codebase
	cargo fmt --all

lint: ## Run Clippy linter and fail on warnings
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt lint test ## Run all checks (format, lint, test) before pushing

clean: ## Clean the target directory
	cargo clean
