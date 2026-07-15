# elephant — build harness (layout per ../zetl conventions)

PREFIX ?= $(HOME)/.local

.PHONY: all build test check lint clippy fmt fmt-fix fuzz bench install uninstall dist clean doc doc-open help

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

# Build the release binary and stage it as a distributable artifact for the
# host platform: dist/elephant-<os>-<arch> plus its .sha256 checksum. The
# release pipeline (.woodpecker/release.yaml) produces all four platforms and
# uploads them to https://files.anuna.io/elephant/; use this only to stage a
# single platform by hand (see scripts/install.sh).
dist: build ## Stage dist/elephant-<os>-<arch> + .sha256 for the host platform
	@set -e; \
	os=$$(uname -s); arch=$$(uname -m); \
	case "$$os" in \
	  Darwin) os=darwin ;; \
	  Linux) os=linux ;; \
	  *) echo "dist: unsupported OS: $$os" >&2; exit 1 ;; \
	esac; \
	case "$$arch" in \
	  arm64|aarch64) arch=arm64 ;; \
	  x86_64|amd64) arch=x64 ;; \
	  *) echo "dist: unsupported architecture: $$arch" >&2; exit 1 ;; \
	esac; \
	artifact="elephant-$$os-$$arch"; \
	mkdir -p dist; \
	cp target/release/elephant "dist/$$artifact"; \
	if command -v sha256sum >/dev/null 2>&1; then \
	  (cd dist && sha256sum "$$artifact" > "$$artifact.sha256"); \
	else \
	  (cd dist && shasum -a 256 "$$artifact" > "$$artifact.sha256"); \
	fi; \
	echo ""; \
	echo "Staged dist/$$artifact and dist/$$artifact.sha256"

clean:
	cargo clean

doc:
	cargo doc --no-deps

doc-open:
	cargo doc --no-deps --open

help: ## List targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'
