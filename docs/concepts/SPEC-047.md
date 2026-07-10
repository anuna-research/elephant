# SPEC-047

zetl's strawman specification "Loro CRDT Store + P2P Realtime Sync Daemon
with DHT-Bootstrapped SPAKE2 Pairing" (branch `spec/047-loro-p2p`,
`specs/SPEC-047-loro-p2p-realtime-sync.md`, v0.9.0-strawman, not yet
approved). elephant-3000's [[SPEC-002-elephant-p2p]] inherits its
load-bearing patterns: routing/secret split of the pairing phrase
(ADR-473), pkarr rendezvous + durable discovery, iroh QUIC transport with
roster gating, Loro version-vector delta sync, opaque auth failures, and
the ≥20-bit PAKE entropy floor analysis (NFR-475). Its open Tier-1 crypto
review (active-adversary analysis of the split) is inherited as an open
item, not assumed resolved.
