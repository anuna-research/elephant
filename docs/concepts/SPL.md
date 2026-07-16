# SPL

Spindle Lisp — the S-expression surface language of
[[Defeasible Logic]] theories in spindle-rust: `(given …)`, `(always …)`,
`(normally …)`, `(except …)`, `(prefer …)`, `(claims …)`, `(trusts …)`,
`(decays …)`, `(threshold …)`, `(meta …)`, with modal wrappers
(`must`/`may`/`forbidden`), Allen-interval temporal predicates
(`during`, `moment`), and arithmetic in rule bodies. Authoritative
grammar: `spindle-rust/docs/src/reference/spl.md`; the sole recogniser in
elephant is `spindle_parser::parse_spl`
([[SPEC-001-elephant-core#CON-001]]). SPL sentences are the propositional
content of every [[Speech Act]] on the wire.

A defeater `(except …)` blocks its head's *complement*: to block a positive
`q`, write `(except d cond (not q))`, not `(except d cond q)`; the head names
the attacking side and is never itself derived (see
[[Defeasible Logic#Defeater polarity]]). The legacy `~>` spelling is rejected.
