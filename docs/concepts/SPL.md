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
