.PHONY: help build build-release test run run-latest run-specific clean

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

run: ## Run the TUI (debug mode) fetching the latest run for the default job
	cargo run

run-latest: build-release ## Run the TUI (release mode) fetching the latest run for the default job
	./target/release/dbt-log-tui

run-specific: build-release ## Run the TUI (release mode) against the specific run ID from the plan (52843050)
	./target/release/dbt-log-tui 52843050

clean: ## Clean the target directory
	cargo clean
