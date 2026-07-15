---
id: BUG-002
title: closure bench corpus fails merge-time validation — NFR-001 gate measured the quarantine path
status: resolved
severity: S3
priority: P2
reported-by: agent (Claude Fable 5, IMPL-005 session)
date: 2026-07-15
---

# BUG-002 — closure bench quarantined its whole corpus

**Severity:** S3 · **Priority:** P2 · **Status:** resolved

## Specification Reference

- Violates: [[SPEC-001-elephant-core#NFR-001]] verification intent (the
  [[SPEC-001-elephant-core#TEST-025]] bench gate)
- Related: [[SPEC-005-elephant-vocabulary#NFR-401]] (the vocab bench that
  surfaced it)

## Environment

- `benches/closure.rs`, all runs since its introduction.
- Detected while adding the SPEC-005 `vocab_view` bench: 57 ns for a
  1 000-entry view was implausible.

## Steps to Reproduce

1. Build the bench corpus with `did = "did:crdt:9999"` and `node_id: 9`.
2. Run `closure::close` over it.
3. Observe `admitted=0 quarantined=1000 conclusions=0`.

## Expected Behaviour

The NFR-001/NFR-003 bench measures the reasoning pipeline over an
admitted corpus.

## Actual Behaviour

Every entry failed merge-time validation (the HLC `node_id` must be
`node_id_from_pubkey(signing key)`, and the DID must resolve), so the
bench measured signature-rejection + quarantine — closure numbers
reported against NFR-001 were for the wrong path.

## Root Cause

- **Category:** test-gap
- **Analysis:** the bench fixture predates the merge-time validation
  hardening and was never re-checked against it; criterion reports
  timings regardless of admission, so nothing failed loudly.

## Resolution

- **Fix:** bench corpus now derives the DID and HLC node id from the
  signing key (same discipline as the test fixtures); the sanity check
  `admitted == n` was verified manually (1 000 admitted, 2 002
  conclusions; vocab view 2.3 ms).
- **Verified by:** re-run of `cargo bench` post-fix; the SPEC-005
  [[SPEC-005-elephant-vocabulary#TEST-408]] bench (`vocab_view`) now
  measures real work.
- **Regression test added:** no — criterion benches are not asserted in
  CI today; adding an `assert!(quarantined.is_empty())` guard inside the
  bench setup is the cheap follow-up (owner HOC).
