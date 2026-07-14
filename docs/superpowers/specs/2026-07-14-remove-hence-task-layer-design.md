# Design: remove the hence task layer from the elephant CLI

Date: 2026-07-14
Status: approved in discussion, pending spec review
Decision owner: HOC

## Context

The CLI carries ~29 top-level verbs. Nine of them — `claim`, `unclaim`,
`complete`, `block`, `unblock`, `join-as`, `next`, `board`, `info` — are
the plan-coordination layer ported from hence 0.7 (SPEC-003), plus two
attachments: `theory create --template plan` and the `assert --task`
flag (the latter is parsed but never read — already dead).

The motivations for removal, all four confirmed: maintenance burden,
unused in practice, conceptual purity, and a crowded help surface. The
canonical demo (`scripts/demo.sh`, the Elephant 2000 release-gate story)
uses none of the nine verbs; the product's vocabulary is speech acts
plus the spindle query surface.

**Succession reframed.** elephant-3000 remains hence's successor, but a
successor need not be backwards compatible. The succession carries the
ideas (shared defeasible theories, signed lifecycle acts) forward; it
does not carry the hence 0.7 verb surface or its byte-frozen SPL
vocabulary as a compatibility commitment.

## Decision

Delete the nine task verbs, `src/tasks.rs`, the plan template, and the
dead `assert --task` flag. Keep the entire spindle query surface
(`status`, `explain`, `why-not`, `require`, `what-if`, `commitments`,
`log`, `watch`, `describe`, `trace`, `dag`) untouched.

## Alternatives considered

- **A. Extract as a companion `hence` binary** (workspace crate driving
  elephant theories). Preserves the workflow and hence-0.7 port path at
  the cost of a second binary, workspace overhead, and a stable API
  boundary. Rejected: keeps surface alive that is not used, and the
  succession no longer requires compatibility.
- **C. Regroup under `elephant plan <verb>`.** Shrinks help only;
  maintenance and conceptual-purity motivations untouched, and it
  half-reverses ADR-205 (flat verbs). Rejected.

## What is removed

- `src/cli.rs`: the nine `Command` variants and their dispatch arms;
  the `--template` flag on `TheoryCmd::Create`; the `task` field on
  `AssertArgs`.
- `src/tasks.rs` (~600 lines) and its `mod` declaration.
- `tests/tasks_cli.rs` (12 tests).
- `src/queries.rs:480` re-export that existed only for `board`/`next`.
- README section "The task layer (hence-compatible)".
- Help-text comparison to hence's `board --dag` in the `dag` verb
  (reword, keep the verb).

Resulting surface: 19 top-level entries, down from 28 — the noun groups
`id` (2), `theory` (6, minus the `--template` flag), `daemon` (4), plus
five speech-act producers and eleven query verbs, all flat (ADR-205).

## What is kept (semantics)

- Corpora are append-only; existing theories containing hence lifecycle
  SPL keep converging identically. Task states remain derivable via
  `status`, `dag`, `describe`. No migration, no data change.
- SPEC-005's predicate registry keeps the lifecycle predicates
  *reserved* for now — reserving names costs nothing and keeps old
  corpora interpretable. A future SPEC-005 revision may unreserve them;
  that is out of scope here.

## Spec surgery

SPEC-003 stays alive (SPEC-005 wikilinks into it ~15 times; `store.rs`
cites ADR-202, the whole CLI cites ADR-205, `describe`/`trace` cite
REQ-208 — all load-bearing). Changes, as a 0.3.0 bump:

- New **ADR-206** recording this decision: the task-verb surface is
  removed; **supersedes ADR-201** — hence 0.7 backwards compatibility
  is no longer a goal. Rationale: successor in ideas, not in surface.
- REQ-201..207, 209, 210 marked **retired** (not deleted, so existing
  anchors keep resolving). REQ-208 (`describe`/`trace`) stays normative.
- ADR-202 (bundles = batched Entries) and ADR-205 (flat verb surface)
  stay normative — surviving code depends on both.
- Orientation section rewritten: elephant-3000 succeeds hence without a
  compatibility contract; §1 "Compatibility contract with hence" is
  demoted to historical record.
- SPEC-005 touch-up: notes referencing ADR-201's freeze as *the* reason
  predicates are reserved get a one-line update (reserved by SPEC-005
  registration; the ADR-201 freeze is superseded).

## Error handling

No new error paths. Removed verbs fall out of clap's parser, so invoking
them yields the standard unknown-subcommand usage error (exit: Usage).

## Testing / gate

- `cargo test` green after deleting `tests/tasks_cli.rs`; check
  `clig_cli.rs` and `queries_cli.rs` for assertions on removed help
  text or verbs (queries_cli.rs:221 cites ADR-205/REQ-208 — those verbs
  survive, so likely fine; verify).
- `cargo clippy` clean; `cargo fmt` unchanged.
- `scripts/demo.sh` runs unmodified (it never used the task layer).
- `zetl -d specs check`: no *new* dead links introduced by the spec
  edits (pre-existing backlog of 55+ stays as-is).

## Aftermath

- Update project memory: the hence-succession framing changed on
  2026-07-14 — succession without backwards compatibility; task layer
  removed.
- The door stays open: a future task-coordination layer can be designed
  natively on elephant (SPEC-003's deferred ADR-204 ideas) without any
  hence-0.7 compatibility constraint.
