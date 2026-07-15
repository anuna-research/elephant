---
id: BUG-003
title: README near-miss example is not reproducible; cold-cache first-assert behaviour undocumented
severity: S3
priority: P2
status: resolved
---

# BUG-003: README near-miss example is not reproducible; cold-cache first-assert behaviour undocumented

**Severity:** S3 (a headline documented feature does not behave as the docs
show; no data-integrity impact — the feature itself is correct)
**Priority:** P2
**Status:** resolved (documentation defect; code is correct)
**Reported by:** exploratory CLI walkthrough (2026-07-15), following the
README `## Quick start` → Vocabulary block verbatim.

## Specification Reference

- Feature under test: [[SPEC-005-elephant-vocabulary#REQ-406]] (near-miss
  advisory on assert), family resolution
  [[SPEC-005-elephant-vocabulary#CON-402]].
- Cold-cache behaviour is specified by
  [[SPEC-005-elephant-vocabulary#NFR-402]] — the code is compliant; only the
  user-facing docs omit it.

## Steps to Reproduce

### Part A — README example yields the wrong advisory kind

Run the README `README.md` Vocabulary block verbatim (daemon up):

```
elephant define ci-green/1 --arg task:symbol \
    --desc "CI pipeline green for task ?t" --kind evidence --asserter role:ci -t release
elephant assert 'ci-green-m2' -t release
```

### Part B — advisory silently absent on the first assert after a daemon (re)start

```
elephant daemon start
elephant theory create T
elephant assert '(normally r ci-green-m1 verified)' -t T
elephant assert 'ci-green-aa' -t T        # advisory fires (view warm)
elephant daemon stop && elephant daemon start
elephant assert 'ci-green-bb' -t T        # NO advisory (cold view)
elephant assert 'ci-green-cc' -t T        # advisory fires again
```

## Expected Behaviour

Per the README the assert in Part A prints:

```
advisory (sibling): family ci-green
  did you mean ci-green-m1?  (listener r-verified)
```

## Actual Behaviour

- **Part A** prints an **inert-family** advisory keyed on the whole atom, not
  the documented **sibling** advisory:

  ```
  assert  s-…
  advisory (inert-family): family ci-green-m2
  ```

  The README snippet also never creates the `r-verified` listener rule it
  refers to, nor declares the `m1`/`m2` tasks — so a copy-paste user cannot
  reach the documented output at all.

- **Part B**: the first assert to a theory after any daemon start emits no
  advisory; the next one does. Correct per NFR-402 but undocumented and
  surprising — it reads as "the feature is broken."

## Root Cause

- **Category:** documentation defect (two parts); the runtime is correct.
- **Analysis (Part A):** the flat atom `ci-green-m2` resolves to its own
  family `Legacy("ci-green-m2")` via `family()` CON-402 rule 4
  (`src/core/vocab.rs:406-427`), so it is never a *sibling* of `ci-green-m1`.
  The **sibling** advisory (`src/core/vocab.rs:1413-1438`) requires: (i) the
  asserted family has a listener (`Body`/`Goal` role), and (ii) a same-family
  demanded-but-unproven sibling exists. Reaching a shared `ci-green` family
  for both `ci-green-m1` and `ci-green-m2` needs `m1`/`m2` declared as tasks
  (so rule 3 strips the suffix), plus an `r-verified` rule listening on the
  unproven `ci-green-m1`. The README shows none of this setup. The
  parameterised form does work (verified end-to-end):

  ```
  elephant assert '(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))' -t T
  elephant assert '(given (review-approved m1))' -t T
  elephant assert '(given (ci-green m2))' -t T
  #   advisory (sibling): family ci-green/1
  #     did you mean (ci-green m1)?  (listener r-verified)
  ```

- **Analysis (Part B):** `compute_advisory` reads the cached `VocabView` and
  returns `None` on a cold miss (`src/daemon/api.rs:428-437`). The cache is
  populated only by `merge_and_close` → `recompute_locked`, the watch seed,
  or a sync refresh — never at daemon start — so the first append to any
  theory in a daemon's lifetime is served against a cold view. This is
  exactly [[SPEC-005-elephant-vocabulary#NFR-402]]'s degradation clause.

## Resolution (proposed — not yet applied)

1. **Docs (Part A):** replace the README Vocabulary example with a sequence
   that actually produces a sibling advisory — the parameterised form above
   is the smallest self-contained one — or keep the flat form and add the
   missing `task-m1`/`task-m2` declarations and the `r-verified` listener.
2. **Docs (Part B):** add one sentence to the README daemon section and to
   NFR-402's user-facing surface noting that the advisory is best-effort and
   absent on the first assert to a theory after a daemon (re)start.
3. **Optional enhancement:** seed reference views for all known theories at
   daemon start (bounded, off the producer path) so the first assert after a
   restart is warm. Preserves the NFR-402 "never delay the append" contract.

## Resolution (applied 2026-07-15)

Documentation-only fix in `README.md` (the code was already correct):

- **Part A:** replaced the flat `assert 'ci-green-m2'` example with a
  self-contained parameterised sequence that actually produces a sibling
  advisory — `define ci-green/1`, a rule listening on `(ci-green ?t)`, a proven
  `(review-approved m1)` witness, then `assert '(given (ci-green m2))'`. The
  `vocab` sample output was also corrected to the real rendering.
- **Part B:** added a blockquote noting the advisory is best-effort and
  daemon-only, and silently absent on a direct-store assert or on the first
  assert to a theory after a daemon (re)start (NFR-402).

Verified by a clean-room run of the exact README commands: the `vocab` table and
the `advisory (sibling): family ci-green/1 … did you mean (ci-green m1)?
(listener r-verified)` output both reproduce verbatim. The optional view-warming
enhancement (item 3) is left for a follow-up and is not part of this fix.
