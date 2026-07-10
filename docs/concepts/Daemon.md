# Daemon

`elephantd` — the long-lived process (same binary, `elephant daemon run`)
that owns the store, holds [[Loro]] replicas of every joined
[[Logical Theory]], keeps [[pkarr]] discovery records fresh, maintains
iroh QUIC sessions with rostered peers, merges and validates incoming
Entries, recomputes closure, and pushes watch events to CLI clients over
a 0600 unix control socket ([[SPEC-002-elephant-p2p#CON-101]]). One-shot
CLI commands proxy through it when it is live and fall back to direct
store access when it is not ([[SPEC-002-elephant-p2p#REQ-102]]).
