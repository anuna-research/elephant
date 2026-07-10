# Loro

[Loro](https://loro.dev) (crates.io `loro`, v1.13.x) is the CRDT engine
elephant uses as the replicated corpus store — one `LoroDoc` per
[[Logical Theory]].

Properties elephant relies on:

- **Synchronous, thread-safe, no async runtime.** `LoroDoc` is Arc-based and
  cheap to clone; the [[Daemon]] owns docs and shares handles.
- **Unique peer id per replica.** `doc.set_peer_id(...)` — elephant derives
  the Loro peer id from the agent's [[DID]] key so replicas never collide.
- **Delta sync via version vectors.** Steady-state exchange is
  `doc.export(ExportMode::updates(&peer_vv))` → `doc.import(&bytes)`;
  snapshots only for bootstrap.
- **Push hook.** `doc.subscribe_local_update` yields encoded update bytes on
  every local commit — the transport broadcasts exactly those bytes.
- **Import events.** `subscribe_root` events carry
  `triggered_by.is_import()`, letting the closure engine recompute only on
  remote change.

Elephant stores the corpus as an append-only list container of signed
message blobs (see [[SPEC-001-elephant-core#CON-002]]): the
G-set-of-signed-deltas model from the prior elephant design, realised on a
general CRDT engine. Loro's richer containers (Map/Text) are available but
the corpus deliberately uses only grow-only insertion — merge semantics stay
CALM, and closure remains a pure function of (corpus, trust).
