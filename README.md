---
title: elephant
mode: explanation
---

# elephant

Elephant lets humans and agents coordinate through signed [[Speech Act|speech acts]] in shared, encrypted [[Logical Theory|logical theories]].
Rules derive conclusions and promise fulfilment from evidence, so collaborators can inspect why a claim holds.

## Quick Start

Install a prebuilt binary on macOS or Linux:

```sh
curl https://files.anuna.io/elephant/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
```

Check the installed binary:

```sh
elephant --version
```

[[first-theory]] walks through identity setup, an approval rule, and a promise that evidence fulfils.
For an existing identity, `elephant info` reports its store and theories.

## Usage

The guides below cover local work, shared theories, and agent setup:

- [[how-to-coordinate-work]] — discover ready tasks, inspect provenance, and record acceptance evidence.
- [[how-to-share-a-theory]] — invite another identity and enable continuous peer sync.
- [[how-to-run-elephant-on-login]] — keep a macOS daemon running across logins.
- [[how-to-install-agent-guidance]] — install the bundled skill with `elephant skill init`.
- [[spindle-integration]] — arithmetic, aggregation, typed reasoning, and shared extensions.

## Architecture

The CLI and [[Daemon]] manage storage, identity, transport, and encryption.
The pure core derives conclusions from a [[Corpus]], trust inputs, and an explicit evaluation time.

[[architecture|Elephant architecture]] explains the boundaries and dependency choices.
[[project-status|Project status]] records implementation limits and open review work.

## API Reference

`elephant --help` lists commands; each subcommand exposes its arguments through `--help`.
Source builds after `v0.1.7` also expose `elephant capabilities --json` for reasoning interfaces and supported fragments.

The reference and contracts cover these interfaces:

- [[inspection]] — proof states, closure fingerprints, and journal accounting.
- [[SPEC-001-elephant-core#CON-001]] — accepted SPL payload grammar.
- [[SPEC-001-elephant-core#CON-002]] — signed journal entries.
- [[SPEC-001-elephant-core#CON-003]] — corpus-to-reasoner translation.
- [[SPEC-002-elephant-p2p#CON-101]] — daemon control API.

Rust API documentation is generated with `make doc` into `target/doc/elephant/index.html`.

## Development

Source builds require Rust and sibling checkouts of `cbcl-rs`, `spindle-rust`, and `did-crdt`.
[[how-to-build-and-check]] covers dependency pins, local installation, tests, benchmarks, fuzzing, and releases.

For specification work, use the repository's linked requirements and tests before changing behaviour.
[[SPEC-001-elephant-core]] defines the core; [[SPEC-006-elephant-next]] defines theory-derived task discovery.
[[CHANGELOG]] records releases and unreleased changes.

## License

Apache-2.0. The repository's `LICENSE` file contains the license text.
