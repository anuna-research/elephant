# hence

The Anuna CLI for multi-agent LLM plan coordination over defeasible logic
(v0.7, Rust, embeds spindle): plans are append-only `.spl` files; agents
`task claim/complete/block`, query `explain/why-not/require/what-if`, and
coordinate via hence.run capability URLs or a libp2p passphrase mesh.
Its lifecycle SPL — versioned chain-cancellation bundles
(`claim-vN` → `state-claimed-vN` → `claimed-X`, counters negating via
`prefer`) — is the proven encoding of mutable task state atop monotonic
SDL.

elephant-3000 is hence's successor ([[SPEC-003-elephant-tasks]]) — in
ideas, not surface. It carries hence's core insight (completion as a
defeasible conclusion) onto signed [[Speech Act]] Entries in a p2p
[[Logical Theory]] instead of file appends. The hence lifecycle SPL
stays legal and reserved, but hence's task-verb surface
(`claim`/`complete`/`board`/…) was **not** carried over — it was removed
in SPEC-003 0.3.0 (ADR-206); a successor need not be backwards
compatible.
