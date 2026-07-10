# Commitment

McCarthy's first-class abstract object ([[Elephant 2000]] §5–6): created
by `make commitment`, destroyed by `cancel`, tested by `exists`, with the
axiom

> exists(t, c) ≡ ∃t′<t. arises(t′, c) ∧ ∀t″∈(t′,t). ¬revoke(t″, c)

A **promise** is an internal commitment *plus* a truthful assertion that
the commitment exists. A reservation *is* a commitment to admit the
passenger — no database row needed; existence is a function of history.

In elephant a commitment arises from a [[cbcl-elephant]] `commit`
performative `(commit sentence-id trigger-conditions goal)`, is revoked
by the same signer's `retract`, and its state (`pending`, `outstanding`,
`fulfilled`, `violated`, `retracted`) is computed by the pure core per
[[SPEC-001-elephant-core#REQ-015]] from the [[Corpus]] and the closure
tags of its trigger and goal. `elephant commitments` renders the ledger;
keeping one's promises is thereby publicly checkable — the paper's
"intrinsic correctness" made operational.
