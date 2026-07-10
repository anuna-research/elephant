# elephant — build harness (layout per ../zetl conventions)

PREFIX ?= $(HOME)/.local

.PHONY: all build test check lint clippy fmt fmt-fix fuzz bench install uninstall clean doc doc-open help

all: check build

build: ## Release build
	cargo build --release

test: ## Run all tests
	cargo test

check: test lint ## Tests + lint

lint: ## rustfmt --check + clippy -D warnings
	cargo fmt -p elephant -- --check
	cargo clippy --all-targets -- -D warnings

clippy:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt -p elephant -- --check

fmt-fix: ## Apply rustfmt
	cargo fmt -p elephant

bench: ## Criterion benches (NFR gates)
	cargo bench

install: build ## Install to $(PREFIX)/bin
	install -d $(PREFIX)/bin
	install -m 755 target/release/elephant $(PREFIX)/bin/elephant

uninstall:
	rm -f $(PREFIX)/bin/elephant

clean:
	cargo clean

doc:
	cargo doc --no-deps

doc-open:
	cargo doc --no-deps --open

help: ## List targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'
