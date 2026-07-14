# Remove the hence task layer — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Delete the nine hence task verbs, `src/tasks.rs`, the plan template, and the dead `assert --task` flag from the elephant CLI, keeping the whole spindle query surface intact.

**Architecture:** Pure removal + spec surgery. The lifecycle SPL stays legal in corpora (append-only), so no data migration. SPEC-003 shrinks (0.3.0, ADR-206 supersedes ADR-201) rather than being deleted, because SPEC-005 links into it and `store.rs`/`queries.rs`/the CLI cite its surviving ADRs.

**Tech Stack:** Rust 2024, clap, cargo test/clippy/fmt.

## Global Constraints

- Corpus is append-only; no data change, no migration.
- `cargo test` green, `cargo clippy` clean, `cargo fmt` unchanged.
- `scripts/demo.sh` runs unmodified (never used the task layer).
- Surviving spec anchors (ADR-202, ADR-205, REQ-208) must keep resolving.

---

### Task 1: Remove the task-layer code

**Files:**
- Modify: `src/cli.rs` — delete 9 `Command` variants (`Board`, `Info`, `JoinAs`, `Next`, `Claim`, `Unclaim`, `Complete`, `Block`, `Unblock`) and their dispatch arms; delete `--template` from `TheoryCmd::Create` + its match handling in `handle_theory`; delete `task` field on `AssertArgs`.
- Modify: `src/lib.rs:23` — delete `pub mod tasks;`
- Delete: `src/tasks.rs`
- Delete: `tests/tasks_cli.rs`
- Modify: `src/queries.rs:480` — the `conclusions_positive` fn stays (used by `p2p/run.rs`, `join_ceremony.rs`); reword its "Re-export for board/next" comment.
- Modify: `dag` verb help in `cli.rs` — reword the "analogue of hence's `board --dag`" line (the `board` verb is gone).

- [ ] Delete `tests/tasks_cli.rs` and `src/tasks.rs`.
- [ ] Remove `pub mod tasks;` from `src/lib.rs`.
- [ ] Strip the 9 variants, dispatch arms, `--template`, `AssertArgs.task`, reword `dag` help + `queries.rs` comment.
- [ ] `cargo build` — expect clean (no references to `tasks::` remain).
- [ ] `cargo test` — expect green (task tests gone; survivors untouched).
- [ ] `cargo clippy --all-targets` clean; `cargo fmt --check` clean.
- [ ] Commit: `feat: remove hence task-layer verbs from the CLI`.

### Task 2: Spec surgery (SPEC-003 → 0.3.0, SPEC-005 touch-up)

**Files:**
- Modify: `specs/SPEC-003-elephant-tasks.md` — version 0.3.0; add ADR-206 (removes the task-verb surface, supersedes ADR-201, succession is in ideas not surface); mark REQ-201..207, 209, 210 retired (keep anchors); rewrite Orientation + demote §1 "Compatibility contract" to historical; changelog entry.
- Modify: `specs/SPEC-005-elephant-vocabulary.md` — update the ~3 lines that cite ADR-201's *freeze* as the reason predicates are reserved (now: reserved by SPEC-005 registration; ADR-201 superseded). Keep all wikilinks resolving.
- Modify: `README.md` — delete the "### The task layer (hence-compatible)" section (lines ~93-105).

- [ ] Edit SPEC-003: header, ADR-206, retire REQs, rewrite orientation/§1, changelog.
- [ ] Edit SPEC-005 freeze-rationale lines.
- [ ] Delete README task-layer section.
- [ ] `zetl -d specs check --dead-links` — no *new* dead links vs. baseline.
- [ ] Commit: `docs: retire the hence task surface in SPEC-003 (0.3.0, ADR-206)`.

### Task 3: Memory + finish

- [ ] Update project memory: succession reframed 2026-07-14 (successor in ideas, not backwards-compatible; task layer removed).
- [ ] Verify full gate one more time; report.
