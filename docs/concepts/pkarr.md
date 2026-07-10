# pkarr

[pkarr](https://pkarr.org) (crates.io `pkarr`, v6.x) publishes signed DNS
resource-record packets keyed by an Ed25519 public key on the BitTorrent
[[Mainline DHT]] — "public-key addressable resource records". No servers,
no registration; anyone can resolve `pk:<z-base32 pubkey>`.

Elephant uses it twice, exactly as [[SPEC-047]] does for zetl:

1. **Invite rendezvous.** The invite code's public routing component derives
   (via HKDF) a throwaway rendezvous keypair; the inviter publishes a
   short-TTL packet under it carrying only an endpoint hint. The joiner
   derives the same keypair from the code and resolves the packet.
2. **Steady-state peer discovery.** Each agent republishes its current
   transport endpoint under a durable per-agent discovery key, so peers
   survive IP changes without any tracker.

Trust rule: a resolved packet is an unauthenticated *hint*. It confers no
authority — [[SPAKE2]] (at join) or membership verification (at reconnect)
must succeed before any application frame is processed.

Operational notes: records expire in hours → the [[Daemon]] republishes
periodically using `resolve_most_recent` + compare-and-swap timestamps;
packets are capped at 1000 bytes; publish/resolve are async (tokio) with
relay fallback for environments that cannot reach the DHT directly.
