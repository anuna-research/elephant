---
title: elephant
mode: explanation
---

# elephant

Elephant lets humans and agents coordinate through signed [speech acts](docs/concepts/Speech%20Act.md) in shared, encrypted [logical theories](docs/concepts/Logical%20Theory.md).
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

[Your first theory and promise](docs/tutorial/first-theory.md) walks through identity setup, an approval rule, and a promise that evidence fulfils.
For an existing identity, `elephant info` reports its store and theories.

## Usage

The guides below cover local work, shared theories, and agent setup:

- [How to coordinate work from a theory](docs/how-to/how-to-coordinate-work.md) — discover ready tasks, inspect provenance, and record acceptance evidence.
- [How to share a theory](docs/how-to/how-to-share-a-theory.md) — invite another identity and enable continuous peer sync.
- [How to run Elephant on login](docs/how-to/how-to-run-elephant-on-login.md) — keep a macOS daemon running across logins.
- [How to install agent guidance](docs/how-to/how-to-install-agent-guidance.md) — install the bundled skill with `elephant skill init`.
- [Spindle reasoning in Elephant](docs/spindle-integration.md) — arithmetic, aggregation, typed reasoning, and shared extensions.

## Architecture

The CLI and [Daemon](docs/concepts/Daemon.md) manage storage, identity, transport, and encryption.
The pure core derives conclusions from a [Corpus](docs/concepts/Corpus.md), trust inputs, and an explicit evaluation time.

[Elephant architecture](docs/explanation/architecture.md) explains the boundaries and dependency choices.
[Project status](docs/explanation/project-status.md) records implementation limits and open review work.

## API Reference

`elephant --help` lists commands; each subcommand exposes its arguments through `--help`.
Source builds after `v0.1.7` also expose `elephant capabilities --json` for reasoning interfaces and supported fragments.

The reference and contracts cover these interfaces:

- [Inspection interfaces](docs/reference/inspection.md) — proof states, closure fingerprints, and journal accounting.
- [CON-001](specs/SPEC-001-elephant-core.md#user-content-con-001-spl-payload-grammar) — accepted SPL payload grammar.
- [CON-002](specs/SPEC-001-elephant-core.md#user-content-con-002-entry-the-corpus-element) — signed journal entries.
- [CON-003](specs/SPEC-001-elephant-core.md#user-content-con-003-closure-pipeline-pure-core) — corpus-to-reasoner translation.
- [CON-101](specs/SPEC-002-elephant-p2p.md#user-content-con-101-control-protocol) — daemon control API.

Rust API documentation is generated with `make doc` into `target/doc/elephant/index.html`.

## Development

Source builds require Rust and sibling checkouts of `cbcl-rs`, `spindle-rust`, and `did-crdt`.
[How to build and check Elephant](docs/how-to/how-to-build-and-check.md) covers dependency pins, local installation, tests, benchmarks, fuzzing, and releases.

For specification work, use the repository's linked requirements and tests before changing behaviour.
[SPEC-001-elephant-core](specs/SPEC-001-elephant-core.md) defines the core; [SPEC-006-elephant-next](specs/SPEC-006-elephant-next.md) defines theory-derived task discovery.
[CHANGELOG](CHANGELOG.md) records releases and unreleased changes.

## License

Apache-2.0. The repository's `LICENSE` file contains the license text.
