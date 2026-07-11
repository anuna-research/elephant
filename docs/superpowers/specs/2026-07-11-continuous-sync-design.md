# Continuous P2P sync — design

Status: accepted (defaults conservative; see Rollout)
Date: 2026-07-11
Relates to: SPEC-002 REQ-106/107/108, CON-103; review finding #6

## Goal

After the join ceremony, replicas of a theory currently diverge — nothing
re-syncs them. This adds the steady-state loop SPEC-002 describes: the daemon
serves incoming sync sessions (roster-gated) and periodically dials known
roster peers, so members reconverge without an operator.

The sync wire is modeled as a small CBCL dialect (`cbcl-elephant-sync`) so the
sync conversation is typed, recognized speech acts — consistent with the
corpus wire and the project's LangSec stance.

## The `cbcl-elephant-sync` dialect

Two performatives, each a `(lang cbcl-elephant-sync …)` form, framed by the
existing `u32-be len ‖ bytes` framing:

```
(lang cbcl-elephant-sync (offer :theory "<id>" :vv "<b64 loro version vector>"))
(lang cbcl-elephant-sync (deliver :theory "<id>" :updates "<b64 loro updates>"))
```

- **offer** announces which theory and what the sender already has (its Loro
  oplog version vector, base64). Replaces the JSON `SyncHello`.
- **deliver** carries the Loro delta bytes the peer lacks (base64). Replaces
  the raw-bytes frame.

Delta *content* stays opaque Loro binary inside a string arg — CBCL types the
control envelope, not the CRDT payload. The grow-only import guard (finding
#3) and the roster gate still apply to the bytes.

Recognizer: a dedicated parser/serializer in `p2p/sync_dialect.rs`, mirroring
how `envelope::parse_wire` recognizes the corpus wire by hand (the runtime
does not run the dialect registry per message). Malformed forms, wrong
dialect name, wrong theory, or bad base64 are refused as `Transport` errors.

### Authentication

Sync control messages are **unsigned**. Authority comes from two facts that
already hold:

1. The iroh QUIC channel authenticates the peer's transport key
   (`Connection::remote_id()`), and that key is an HKDF subkey of the peer's
   identity seed (`transport::transport_secret`).
2. The theory roster binds `did ↔ node_pk` via the signed member fact recorded
   at join (REQ-105).

So "the connection's `remote_id` is in this theory's roster" (REQ-107)
authenticates the peer as a member without a per-message signature.

## Sync session (rewrite of `wire::sync_session`)

Symmetric, unchanged shape, now speaking the dialect:

```
A→B: offer(theory, vv_A)
B→A: offer(theory, vv_B)   — B rejects if theory ≠ its own
A→B: deliver(theory, updates for vv_B)
B→A: deliver(theory, updates for vv_A)   — each side grow-only-imports
```

Convergence property (equal oplog VVs after a round) is preserved and already
covered by the duplex tests.

## Sync engine (`p2p/sync.rs`)

- `roster_node_pks(paths, theory_id) -> HashSet<String>` — runs the closure,
  collects `member "<did>" "<node_pk>"` node_pks. The roster gate lookup.
- `handle_connection(conn, paths, ident)` — accept side: read the opening
  `offer` to learn the theory; refuse unless `conn.remote_id()` is in that
  theory's roster; open the store, run the session, persist under the store's
  write lock, then drain the MLS lane (`process_mls_lane`).
- `dial_once(endpoint, paths, ident)` — for each local theory, for each roster
  peer node_pk, resolve + `dial_sync` + run the session (best-effort; per-peer
  errors are logged, not fatal).
- `run(paths, ident, shutdown, interval)` — bind `sync_endpoint`, spawn the
  accept loop, run the dialer every `interval` until `shutdown`.

Single-writer discipline (REQ-102): the daemon is the one writer. The sync
handler persists imported deltas through a new `TheoryStore::commit_synced`,
which takes the per-theory `write.lock`, re-imports the on-disk snapshot (to
merge any concurrent HTTP append), then flushes — the same lock the append
path uses, so the two never clobber.

## Daemon integration

`daemon::api::serve` spawns `sync::run` as a background task tied to the
existing shutdown `Notify`, using the identity it already loads (today
`_ident`). Endpoint-bind failure is logged and non-fatal — the HTTP control
plane serves regardless.

## Rollout / config

Continuous sync is **opt-in** via `ELEPHANT_SYNC_INTERVAL` (seconds):

- unset or `0` → disabled (default; current behavior, no network binding).
- e.g. `30` → dialer runs every 30s; the accept loop is always on when enabled.

Rationale: the live iroh transport can't be exercised in CI (its loopback
test is `#[ignore]`d for discovery flakiness), so defaulting off keeps the
daemon and its tests deterministic while making the feature one env var away.
The README documents it.

## Testing

Unit / duplex (deterministic, in CI):
- `sync_dialect`: offer/deliver serialize∘parse roundtrip; malformed and
  wrong-dialect forms rejected.
- `sync_session` over an in-memory duplex: two docs converge to equal VVs
  (existing tests, ported to the dialect).
- `roster_node_pks`: a built roster yields the expected node_pks; a non-member
  key is absent (the gate's decision function).
- `commit_synced`: a synced import is durably merged with a concurrent append.

Live (ignored, documented): endpoint bind + dial is covered structurally by
the existing `#[ignore]` transport test; the accept/dial loops are thin glue
over the tested `handle_connection` core.

## Non-goals

- No new discovery mechanism — reuses the roster's `node_pk` + Mainline DHT.
- No delta-level CBCL typing of CRDT ops — deltas stay opaque Loro bytes.
- No signed sync messages — channel + roster gate authenticate.
