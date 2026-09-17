# Changelog

Notable changes to Elephant, newest first. Release history is reconstructed
from Git tags and commits; changes after the latest tag appear under Unreleased.

## Unreleased

### Added

- Spindle v0.4 reasoning integration: arithmetic, integer/symbol aggregation,
  temporal reasoning, and trust diminishment.
- `capabilities` and `reason` commands, including typed `spindle.reason.v2`
  JSON output and trust details through `reason --trust` and `status --trust`.
- Shared, signed lookup functions and named aggregators through
  `extensions set` and `extensions show`, replicated through encrypted theory sync.
- Raw candidate inspection with `abduce` and Spindle vocabulary inspection with
  `vocab --spindle`.

### Changed

- Query preparation uses the closure's reference time and extension registry;
  hypothetical facts are inserted before grounding and aggregation.
- Existing JSON views include additive typed literal fields. The legacy closure
  fingerprint continues to use display strings.
- CI and release builds pin the Spindle interface-parity revision required by
  the integration.

### Documentation

- Added the [Spindle integration guide](docs/spindle-integration.md), including
  supported fragments, extension semantics, and bounded-search limitations.
- Added design specifications for theory references and published reports;
  these documents do not represent implemented features.

## [0.1.7] - 2026-08-09

### Added

- `elephant next -t THEORY [--json]` discovers available work from theory
  conclusions, commitments, and task documentation, including readiness provenance.

### Changed

- Migrated CI and release automation to Forgejo Actions on `git.anuna.io`.

## [0.1.6] - 2026-07-17

### Fixed

- Updated the release pipeline's pinned Spindle revision.
- Release tooling verifies `Cargo.lock` against pinned sibling dependencies
  before tagging.

## [0.1.5] - 2026-07-17

### Fixed

- Reconciled `Cargo.lock` with pinned sibling dependencies so locked release
  builds succeed, removing an incorrect `cbcl-core` dependency edge to `sha2`.

## [0.1.4] - 2026-07-17

### Added

- `info` reports the active store and identity; `show` inspects a journal entry.
- Per-peer and per-theory sync health in `daemon status`.
- Closure fingerprints, canonical proof states, and journal accounting.
- Explanations for blocked literals and ambiguity blocking in `why-not`.

### Fixed

- `require` returns verified fact sets and reports search exhaustiveness accurately.
- `what-if` rejects unsupported rule and preference hypotheticals.
- Retraction log entries have stable identifiers and explicit targets.
- `describe` emits plain JSON metadata values.
- Concurrent, bounded peer dialing prevents offline peers from delaying others.
- Corrected input validation, fingerprinting, and journal accounting.

## [0.1.2] - 2026-07-15

### Fixed

- Pinned CI's `did-crdt` dependency to its canonical repository.

## [0.1.1] - 2026-07-15

First tagged release in this repository.

### Added

- Signed theory journals, defeasible reasoning, queries, and commitments.
- Identity and theory management, a background daemon, continuous peer-to-peer
  sync, invite-based joining, and MLS end-to-end encryption.
- Theory dependency graphs with `dag`, predicate vocabulary inspection and
  documentation with `vocab` and `define`, and near-miss advisories.
- Release tooling and CI for cross-compiled binary distribution.

### Changed

- Consolidated the CLI into a flat verb surface and removed the former `hence`
  task layer.

### Fixed

- Computed the pinned dialect hash from canonical bytes.
- Improved invite and join reliability, membership removal propagation, and
  concurrent rotation handling.

[0.1.7]: https://git.anuna.io/anuna-research/elephant/compare/v0.1.6...v0.1.7
[0.1.6]: https://git.anuna.io/anuna-research/elephant/compare/v0.1.5...v0.1.6
[0.1.5]: https://git.anuna.io/anuna-research/elephant/compare/v0.1.4...v0.1.5
[0.1.4]: https://git.anuna.io/anuna-research/elephant/compare/v0.1.2...v0.1.4
[0.1.2]: https://git.anuna.io/anuna-research/elephant/compare/v0.1.1...v0.1.2
[0.1.1]: https://git.anuna.io/anuna-research/elephant/src/tag/v0.1.1
