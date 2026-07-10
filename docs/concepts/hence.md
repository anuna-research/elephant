# hence

The Anuna CLI for multi-agent LLM plan coordination over defeasible logic
(v0.7, Rust, embeds spindle): plans are append-only `.spl` files; agents
`task claim/complete/block`, query `explain/why-not/require/what-if`, and
coordinate via hence.run capability URLs or a libp2p passphrase mesh.
Its lifecycle SPL — versioned chain-cancellation bundles
(`claim-vN` → `state-claimed-vN` → `claimed-X`, counters negating via
`prefer`) — is the proven encoding of mutable task state atop monotonic
SDL.

elephant-3000 is hence's successor ([[SPEC-003-elephant-tasks]]): the
same SPL vocabulary and board semantics, carried as signed [[Speech Act]]
Entries in a p2p [[Logical Theory]] instead of file appends — realising
the shelved hence-v2 upgrade plan (Ed25519-load-bearing claims, CBCL
wire, partitions, tombstone retraction) on the elephant substrate.
