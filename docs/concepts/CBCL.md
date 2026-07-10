# CBCL

Common Business Communication Language — the safe, self-extending agent
messaging language implemented by
[cbcl-rs](https://codeberg.org/anuna/cbcl-rs) (named for McCarthy's 1982
proposal; Lean-verified invariants). Deliberately restricted to DCFL so
every message is fully recognisable before action. Core: eight
performatives (`tell ask reply ok error cancel hello bye`), wrapper forms
(`signed`, `with-limits`, `lang`), dialect definition via `(define …)` /
`(extend …)`, canonical RFC-9804 S-expression encoding, and invariants
R1 (no recursion), R2 (resource bounds), R3 (core-performative
preservation), R4 (signatures — abstract `Signer` trait, elephant
supplies Ed25519). elephant speaks the [[cbcl-elephant]] dialect and pins
its content hash.
