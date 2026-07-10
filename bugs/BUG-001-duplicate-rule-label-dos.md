---
id: BUG-001
title: Duplicate explicit rule label across entries crashes closure
severity: S2
priority: P1
status: verified
---

# BUG-001: Duplicate explicit rule label across entries crashes closure

**Severity:** S2 (core feature — closure — produces an error for the whole
theory; no per-member workaround)
**Priority:** P1
**Status:** verified
**Reported by:** the [[SPEC-001-elephant-core#TEST-021]] convergence property
test (`tests/convergence_prop.rs`), which the USDD Red-Gate discipline
required before the fix.

## Specification Reference

- Violates: [[SPEC-001-elephant-core#REQ-021]] (closure is a total, pure
  function of the corpus) and its availability corollary — a member must not
  be able to make another member's closure fail.
- Related: [[SPEC-001-elephant-core#CON-003]] stage 4 (SPL assembly).

## Steps to Reproduce

1. Any member asserts `(normally r-x a b)`.
2. Any member (the same or another) asserts `(normally r-x c d)` — same
   explicit label, different or identical body — as a separate Entry.
3. Run `elephant status` (or any closure).

## Expected Behaviour

Closure completes deterministically; REQ-021 requires it to be total.

## Actual Behaviour

The assembled SPL theory contains two `(normally r-x …)` rules, and
`spindle_parser::parse_spl` rejects the duplicate label
(`Duplicate rule label: r-x`), so `close()` returns `Reasoner(...)` and every
query on the theory fails. Because any member can append such an Entry, this
is a member-triggerable denial of service against the whole theory's
closure.

## Root Cause

- **Category:** design-error (spec gap in CON-003 stage 4).
- **Analysis:** rule labels share one namespace across the assembled theory
  (hence's cross-entry references depend on this — a `claim` bundle's
  `r-cl-cancel-vM` references `r-ucl-state-vM` from a *different* Entry), so
  the assembly cannot simply namespace labels per Entry. But it also did not
  guard against two Entries defining the same explicit label.

## Resolution

- **Fix:** `core::closure::close` now tracks explicit rule labels during
  assembly and keeps the FIRST definition by corpus order (hlc, signer),
  dropping later redefinitions from *this closure* while leaving them in the
  corpus and journal. Redefined entries are marked `label_shadowed` and shown
  with status `shadowed` in `elephant log` (REQ-016), so the suppression is
  visible, not silent. Unlabelled facts are unaffected (spindle auto-numbers
  them; auto labels never collide across entries). A cheap textual guard
  (only `always`/`normally`/`except` forms are parsed for a label) keeps the
  extra work off the common fact path (NFR-001).
- **Verified by:** `tests/convergence_prop.rs::permutation_invariant` and
  `::duplication_invariant` (the latter reproduced the crash before the fix);
  determinism holds because `admitted` is in (hlc, signer) order, so
  first-wins is replica-independent.
- **Regression test added:** yes (the two property tests above).

## Follow-up

The semantic question — should a second signer be able to *redefine* a label,
or should redefinition require retraction first? — is deferred to a future
spec revision. First-wins is the safe, deterministic default; it is not a
silent data loss because the shadowed entry is visible in the journal.
