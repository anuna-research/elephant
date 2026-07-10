# cbcl-elephant

The [[CBCL]] dialect shipped at `cbcl-rs/dialects/elephant.cbcl` — "speech-act
dialect for agents that exchange logical sentences", explicitly inspired by
[[Elephant 2000]]. Declared as `(define cbcl-elephant (cbcl)
@logic-agents-consortium …)` with resource bounds
`max-depth 12, max-expansion-size 1024, verification-time 50`.

Seven performatives, each carrying an [[SPL]] payload:

| Performative | Arguments | Speech act |
|---|---|---|
| `assert` | `(sentence-id spl)` | assertion into the theory store |
| `retract` | `(sentence-id reason)` | same-signer withdrawal of a prior sentence |
| `query` | `(literal explanation-requested)` | question — answers are provable/refuted/unknown |
| `concede` | `(literal in-reply-to)` | acceptance of another agent's position |
| `commit` | `(sentence-id trigger-conditions goal)` | promise — creates a [[Commitment]] |
| `request` | `(request-id addressee trigger-conditions goal)` | request for a commitment |
| `justify` | `(conclusion-id premises)` | justification of a conclusion |

All templates expand to core `tell`/`ask` with `:domain elephant`, so R3
(core-performative preservation) holds by construction. The dialect ships
unsigned; elephant-3000 installs it at startup via
`cbcl_parser::parse_dialect` + `DialectRegistry::install` and treats its
hash as part of the protocol version.
