# Defeasible Logic

A nonmonotonic logic of strict rules (`always`), defeasible rules
(`normally`), defeaters (`except`) and superiority (`prefer`), yielding
proof tags +D/+d/-d/-D per literal. elephant embeds
[spindle-rust](https://git.anuna.io/anuna-research/spindle-rust)'s SDL
(ambiguity-blocking) engine: `parse_spl` → `reason()` →
trust-weighted conclusions, plus `explain`, `why_not`, `what_if`, and
bounded abduction.

Two elephant-specific disciplines: every statement fed to the reasoner is
wrapped in a `(claims …)` provenance block derived from its verified
envelope ([[SPEC-001-elephant-core#ADR-012]]) — spindle gives unsourced
rules trust 0.0, so unsigned content can never carry weight; and the SPL
theory stays monotonic — retraction happens *before* the reasoner sees
the theory ([[SPEC-001-elephant-core#ADR-006]]). Closure plays the role
McCarthy assigned to circumscription in [[Elephant 2000]].

## Defeater polarity

A defeater (`except`) **attacks its head; it can never establish it.** To
block a positive conclusion `q`, the defeater's head must be the *complement*
`(not q)` — not `q`:

```
(given p)                 ; a fact
(normally r p q)          ; p ⇒ q, so q is +d
(except   d p (not q))    ; when p holds, block q — head is (not q)
```

With this defeater, `q` moves from `+d` to `-D` (blocked) while `(not q)`
stays `-D`: the defeater cancels `q` without proving its negation. Writing
the head with the *positive* literal — `(except d p q)` — instead attacks
`(not q)` and leaves `q` at `+d`, an inert-for-blocking and silent mistake.
The legacy `~>` spelling for defeaters is rejected; use `except`.
