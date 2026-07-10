# Defeasible Logic

A nonmonotonic logic of strict rules (`always`), defeasible rules
(`normally`), defeaters (`except`) and superiority (`prefer`), yielding
proof tags +D/+d/-d/-D per literal. elephant embeds
[spindle-rust](https://codeberg.org/anuna/spindle-rust)'s SDL
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
