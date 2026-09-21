---
title: How to build and check Elephant
mode: how-to
---

# How to build and check Elephant

This guide builds Elephant from source and runs its repository checks.
Use a Rust toolchain compatible with the pinned dependencies; the package declares a minimum of Rust 1.85.
The repository's `rust-toolchain.toml` selects stable Rust with rustfmt and Clippy.

## Prepare the workspace

Clone `cbcl-rs`, `spindle-rust`, and `did-crdt` alongside the Elephant checkout.
All are hosted under `https://git.anuna.io/anuna-research/`.
Their directories satisfy the relative path dependencies in `Cargo.toml`.

For a build matching CI, use the revisions in `.forgejo/workflows/ci.yaml`.
The release workflow pins the same sibling sources.
`Cargo.lock` pins the dependency graph; it does not pin the contents of sibling directories.

## Build and install

From the Elephant repository, run:

```sh
make build
make install
```

The binary is `target/release/elephant`; installation defaults to `~/.local/bin/elephant`.
For a different prefix, pass `PREFIX=/absolute/path` to `make install`.

The prebuilt installer in [Quick Start](../../README.md#user-content-quick-start) supports macOS and Linux on arm64 and x64.
It checks the downloaded SHA-256 digest and uses `~/.local/bin` by default.
`ELEPHANT_INSTALL_DIR` overrides that destination.

## Run checks

Run the standard checks and task-discovery acceptance scenario:

```sh
make check
make acceptance
```

`make check` runs tests, formatting checks, and Clippy with warnings denied.
`make acceptance` builds the binary and exercises [SPEC-006-elephant-next](../../specs/SPEC-006-elephant-next.md) in a throwaway store.
Daemon tests need permission to bind local sockets.
Live discovery tests remain ignored unless explicitly selected.

For individual suites and performance work, use:

```sh
make test
make bench
cargo +nightly fuzz run invite_code
```

Available fuzz targets are `entry_json`, `spl_payload`, `cbcl_wire`, `invite_code`, and `sealed_entry`.
Fuzzing requires nightly Rust and `cargo-fuzz`.

For specification changes, check the vault:

```sh
zetl check --dead-links --fail-on error
```

For API documentation, run:

```sh
make doc
```

## Release

For an authorized release, run `./release.sh <version>`.
The script checks pinned dependencies, runs release gates, creates a tag, and triggers binary publication through Forgejo Actions.
Inspect its result before announcing a release.
