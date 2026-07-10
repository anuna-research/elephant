---
id: SPEC-002
title: elephant p2p — daemon, SPAKE2 join, pkarr discovery, Loro sync
version: 0.1.0
status: implemented
date: 2026-07-11
last-updated: 2026-07-11
audience: agent, human reviewer
---

# SPEC-002 — elephant p2p

## Orientation

Intent: Keep every member's replica of every joined theory converged —
continuously while the [[Daemon]] runs, eventually otherwise — with no
server, no registry, and membership bootstrapped by nothing more than a
short spoken code.

Metaphor: a diplomatic pouch service — couriers (QUIC connections) found
via a public noticeboard ([[pkarr]] on the [[Mainline DHT]]), but a pouch
is only ever handed to someone who spoke the password at the door
([[SPAKE2]]) or is already on the embassy's list (the roster).

Structure:

```
   elephant CLI ──CON-101 control HTTP────▶ ┌── elephantd ─────────────┐
   (one-shot)   loopback + bearer token     │ store: LoroDocs (CON-002)│
                                            │ closure on merge (SPEC-1)│
   invite code  ─┐                          │                          │
   num-word-word │ num ─HKDF─▶ rendezvous   │  iroh QUIC endpoint      │
   (CON-102)     │            pkarr record  │  NodeId ↔ DID bound      │
                 └ word-word ─▶ SPAKE2 ─────▶  roster gate (REQ-107)   │
                                            └───────┬──────────────────┘
        durable discovery: pkarr record per agent ◀─┘ sync: Loro deltas
        (REQ-106, republished)                        (CON-103, REQ-108)
```

Decisions: [[#ADR-101]] iroh QUIC transport · [[#ADR-102]] symmetric
SPAKE2 join with routing/secret split · [[#ADR-103]] membership as
signed corpus facts · [[#ADR-104]] revocation deferred ·
[[#ADR-105]] corpus E2EE lives in [[SPEC-004-elephant-e2ee]] ·
[[#ADR-106]] loopback-HTTP control plane (hark pattern).

Load-bearing: [[#REQ-104]] join ceremony · [[#REQ-107]] roster gate ·
[[#REQ-108]] delta sync · [[#REQ-110]] single-use invites ·
[[#NFR-104]] invite entropy floor.

Open: active-adversary analysis of the routing/secret split inherited
from [[SPEC-047]] Q1 (owner HOC; Tier-1 human crypto review before any
claim of adversarial soundness) · membership revocation ([[#ADR-104]]) ·
corpus encryption at rest ([[#ADR-105]]).

Detail: [[SPEC-001-elephant-core]] for Entry/closure;
[[SPEC-047]] (zetl, branch `spec/047-loro-p2p`) is the pattern source.

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD
NOT, RECOMMENDED, MAY, and OPTIONAL in this document are to be
interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only
when, they appear in all capitals.

## 1. Requirements

### Daemon

#### REQ-101: Daemon lifecycle

The binary SHALL provide `elephant daemon start|stop|status|run`: `run`
executes in the foreground; `start` daemonises `run`; `stop` terminates
via the control socket; `status` reports PID, uptime, joined theories,
peer counts, and sync/quarantine counters. `start` SHALL be idempotent
(detect a live daemon and report it) and SHALL recover from stale
pid/socket files after a crash. A store lock SHALL prevent two daemons
over one state directory.

Trace: [[#TEST-101]] · [[users/operator/happy-paths#HP-O5]]

#### REQ-102: Control plane

The daemon SHALL serve a loopback-only HTTP control API authenticated by
a per-run bearer token stored 0600 in a discovery record
(`daemon.json` with pid, addr, token, api version — the hark pattern);
an exclusive file lock is the single source of truth for liveness, and
startup SHALL classify prior state (live / stale-lock-free /
stale-lock-held / api-incompatible) before acting. CLI commands SHALL
use the daemon when it is live (single writer to the store) and fall
back to direct store access WITH a clear notice when it is not.

Trace: [[#TEST-102]] · [[#CON-101]]

#### REQ-103: Invite issuance

The CLI SHALL, on `elephant theory invite <theory> [--ttl <dur>]`,
generate an invite code per [[#CON-102]], publish the rendezvous
[[pkarr]] record pointing at this agent's transport endpoint, hold the
pending invite (in the daemon, or in the foreground process when no
daemon runs), and print the code exactly once to the TTY.

Trace: [[#TEST-103]] · [[#CON-102]]

#### REQ-104: Join ceremony

The CLI SHALL, on `elephant theory join <code> [--alias <name>]`:
read the code (argument or TTY prompt; the secret words SHALL never be
logged), derive the rendezvous key from the routing component, resolve
the inviter's endpoint from the DHT, open a transport connection, run
symmetric [[SPAKE2]] with the secret words as password, confirm the key
by HMAC before any payload, receive the sealed introduction (theory id,
genesis Entry, inviter DID document, roster), verify it, create the
local replica, and complete initial sync. On any failure the joiner
SHALL be left with no partial theory state.

Trace: [[#TEST-104]] · [[#CON-102]] · [[#CON-104]] ·
[[users/operator/happy-paths#HP-O2]]

#### REQ-105: Membership admission

Completing [[#REQ-104]] SHALL cause the inviter to append a signed
membership Entry — an `assert` of
`(given (member <joiner-did> <joiner-node-pk>))` — into the theory
corpus, binding the joiner's DID to its transport public key. The roster
of a theory at any time IS the closure-derived set of `member` facts
asserted by the creator or by existing members (chain rooted at the
genesis creator).

Trace: [[#TEST-105]] · [[#ADR-103]]

#### REQ-106: Durable discovery

The daemon SHALL publish, and republish before DHT expiry, a [[pkarr]]
record under the agent's durable discovery key mapping to its current
transport endpoint, and SHALL resolve peers' records to reconnect after
address changes WITHOUT any operator action.

Trace: [[#TEST-106]] · [[#CON-104]]

#### REQ-107: Roster gate

The daemon SHALL accept a sync connection for a theory only from a
transport key bound to a member DID by the theory's roster
([[#REQ-105]]); non-members SHALL be refused before any sync frame is
parsed. A resolved DHT record is an unauthenticated hint and SHALL
confer no access by itself.

Trace: [[#TEST-107]]

#### REQ-108: Corpus sync

Connected member replicas SHALL exchange [[Loro]] version vectors and
ship only missing updates ([[#CON-103]]); after exchange, both oplog
version vectors SHALL be equal. Every remotely-received Entry passes
merge-time validation ([[SPEC-001-elephant-core#REQ-022]]) before it can
influence closure — transport authentication never substitutes for
Entry verification.

Trace: [[#TEST-108]] · [[#CON-103]]

#### REQ-109: Watch via daemon

When the daemon merges remote updates that change a watched literal's
tag or a commitment's state, it SHALL push the change to subscribed
control-socket clients ([[SPEC-001-elephant-core#REQ-017]] stream)
WITHIN one closure cycle.

Trace: [[#TEST-109]]

#### REQ-110: Single-use, expiring invites

An invite SHALL be consumed by the first completed OR failed SPAKE2
attempt and SHALL expire after its TTL (default 15 minutes); the
rendezvous record SHALL be torn down on consumption or expiry. A second
join attempt against a consumed invite SHALL fail closed.

Trace: [[#TEST-110]] · [[#NFR-104]]

#### REQ-111: Opaque auth failure

All join failures after code entry (wrong words, expired, consumed,
non-member) SHALL surface to the remote party as a single opaque
`auth-failed` WITH no distinguishing detail; the local operator gets the
full reason.

Trace: [[#TEST-111]]

### Non-functional

#### NFR-101: Join latency ≤ 30 s from code entry to synced replica for a
corpus ≤ 1 000 Entries on broadband, 95th percentile.

#### NFR-102: Reconnect discovery ≤ 20 s 95p after a peer's address
change (bounded by DHT propagation).

#### NFR-103: Daemon idle footprint ≤ 100 MB RSS and ≤ 1 % CPU with 10
theories × 1 000 Entries. // provisional; measured by TEST-112

#### NFR-104: Invite secret entropy ≥ 20 bits (two words from a
≥ 1024-word list), defensible ONLY together with [[#REQ-110]]'s
single-guess burn — inherited analysis from [[SPEC-047]] NFR-475.

## 2. Architecture decisions

#### ADR-101: Transport = iroh QUIC

**Decision.** Peer connections use iroh: per-agent Ed25519 NodeId,
always-encrypted QUIC, NAT hole-punching, optional public relays as
fallback only.

**Context.** The goal directs pkarr/DHT for *discovery* and points at
[[SPEC-047]], which pairs pkarr discovery with iroh transport; did-crdt's
own sync feature also chose iroh. Alternatives considered: raw QUIC
(quinn) + hand-rolled Noise — rejected, hand-rolled crypto at a trust
boundary; libp2p — rejected, far larger surface for the same need.

**Trade-offs.** (+) encrypted, mutually-authenticated channels keyed by
a public key we can bind to DIDs; NAT traversal solved. (−) heavy
dependency; relay availability is third-party. The iroh NodeId is a
*transport* key, distinct from the DID signing key, bound via
[[#REQ-105]]'s membership fact.

#### ADR-102: Join = symmetric SPAKE2 over a `num-word-word` code

**Decision.** Invite code `NNNN-word-word` ([[#CON-102]]).
Routing/secret split exactly as [[SPEC-047]] ADR-473: the number derives
the rendezvous pkarr keypair via HKDF (public, enumerable, carries no
secret); the two words are the SPAKE2 password and never leave the
machines except by the human channel. Symmetric mode
(`start_symmetric`), identity = `elephant-join-v1:<theory-id-prefix>`;
the 32-byte output feeds HKDF → (confirmation MAC key, introduction
sealing key).

**Trade-offs.** (+) copy-pasteable/speakable code; PAKE single-guess
bound; pattern already Tier-1-reviewed in the zetl lineage (review
pending there — inherited as an open item, not silently assumed sound).
(−) inviter must be reachable during the invite TTL.

#### ADR-103: Membership is corpus evidence, not config

Roster facts `(member <did> <node-pk>)` live in the theory itself as
signed asserts ([[#REQ-105]]) — dogfooding the prior design's
directory-partition principle: membership is auditable, syncs with the
corpus, and is evaluated by the same closure as everything else.
Consequence: who admitted whom is permanent history.

#### ADR-104: Revocation deferred

v0.1 has no member removal: the roster is grow-only, matching the
grow-only corpus. // SIMPLIFY: no revocation; ceiling: first real
multi-tenant deployment or compromised member; upgrade path: epoch'd
roster + connection refusal per SPEC-047 REQ-481 (trace: this ADR).

#### ADR-105: Corpus E2EE via MLS — superseded deferral

Originally this ADR deferred group-key encryption. Superseded the same
day by stakeholder directive: end-to-end encryption is REQUIRED and is
specified in [[SPEC-004-elephant-e2ee]] (MLS group per theory, sealed
entries, keybook, steward commits). Residual scope kept here:
rendezvous and discovery records still contain endpoints only, never
corpus data, and the DHT observer analysis is unchanged.

#### ADR-106: Control protocol = loopback HTTP + bearer token (the hark pattern)

The daemon binds `127.0.0.1:0`, writes an atomic 0600 discovery record
`{pid, addr, token, started_at, version, api_version}` and holds an
exclusive flock for its lifetime; clients authenticate with the record's
32-byte random bearer token. Bodies are RFC 8259 JSON recognised by
serde into closed request/response types (`deny_unknown_fields`, ≤ 1 MiB)
before any action — a declared grammar with a single recogniser. Watch
streams are NDJSON responses. This replaces the earlier unix-socket
sketch because the pattern (including liveness probe classification and
stale-state recovery) is already proven in `../hark`
(`daemon.rs`/`local_api.rs`) — Simplicity Ladder rung 4 — and is
cross-platform for free. CBCL control framing (as [[SPEC-047]] chose)
was considered and rejected for v0.1: the loopback boundary is same-user
local, not the inter-agent trust boundary where CBCL's R1–R5 earn their
weight. // SIMPLIFY: JSON-over-loopback not CBCL; ceiling: remote
control plane; upgrade: swap framing to a CBCL dialect (trace: this ADR).

## 3. Contracts

#### CON-101: Control protocol

Loopback HTTP routes (all under `/v1`, `Authorization: Bearer <token>`):
`GET /v1/ping` · `GET /v1/status` · `POST /v1/theories/{id}/entries`
(pre-signed Entry append) · `POST /v1/invite` · `POST /v1/join` ·
`GET /v1/theories/{id}/watch?literals=…` (NDJSON stream) ·
`POST /v1/stop`. Bodies: RFC 8259 JSON into closed serde types
(`deny_unknown_fields`; the Rust types are the single source of truth).
Responses mirror [[SPEC-001-elephant-core#CON-004]] shapes. Pre: body ≤
1 MiB; valid token; loopback peer. Post: exactly one response per
request; pushes only on watch streams. Error model: JSON error object,
401/400/404 semantics, connection close on framing violation.
Implements: [[#REQ-102]] [[#REQ-109]]. Verified by: [[#TEST-102]]
[[#TEST-113]] (fuzz).

#### CON-102: Invite code

```abnf
code   = number "-" word "-" word
number = 4*5DIGIT   ; routing only: HKDF("elephant/rdv/v1", number ‖ theory-hint)
                    ;   -> rendezvous Ed25519 keypair for the pkarr record.
                    ;   Public, enumerable, no secret (SPEC-047 NFR-475 analysis).
word   = 3*8ALPHA   ; secret: SPAKE2 password = "word-word",
                    ;   EFF/BIP39-style list ≥ 1024 words, ≥ 20 bits total.
```

Recogniser: one Rust function `invite::parse` (regular grammar), used by
both `invite` display and `join` input; anything else is rejected whole.
Codes are single-use ([[#REQ-110]]). The code SHALL never appear in
argv of a remote process, logs, or JSON output; `join` accepts it via
TTY prompt when not given as a local argument.
Implements: [[#REQ-103]] [[#REQ-104]] [[#REQ-110]].
Verified by: [[#TEST-103]] [[#TEST-104]] [[#TEST-113]] (fuzz).

#### CON-103: Sync session

Over an established member connection, per theory: both sides send
`{theory, vv}` (Loro version vector, opaque bytes); each side exports
`ExportMode::updates(&peer_vv)` and streams frames
`u32-be len ‖ loro-update-bytes`; receiver `import_batch`es, then
merge-validates any new Entries (quarantining per
[[SPEC-001-elephant-core#REQ-022]]) and recomputes closure once per
batch. Live phase: `subscribe_local_update` bytes are pushed as they
commit. Pre: connection passed [[#REQ-107]]. Post: equal oplog version
vectors. Error model: malformed frame → connection closed, counters
incremented; import errors never panic the daemon (fuzzed decoder).
Implements: [[#REQ-108]]. Verified by: [[#TEST-108]] [[#TEST-114]]
(property: convergence under partition/reorder).

#### CON-104: pkarr records

Rendezvous record (invite): TXT `_elephant.rdv` =
`v=1;ep=<iroh-node-addr>` under the HKDF-derived keypair, TTL ≤ invite
TTL. Durable record (discovery): TXT `_elephant.agent` =
`v=1;ep=<iroh-node-addr>;did=<did:crdt:…>` under the agent's discovery
keypair (derived from the identity key by HKDF, so one backup restores
both). ≤ 1000 bytes; republish at ~half the DHT expiry;
`resolve_most_recent` + CAS on republish. Records are hints only
([[#REQ-107]]).
Implements: [[#REQ-103]] [[#REQ-106]]. Verified by: [[#TEST-106]].

## 4. Test specification

| TEST | Validates | Positive | Negative-input | Negative-output |
|---|---|---|---|---|
| TEST-101 | REQ-101 | start/status/stop cycle; stale pidfile recovery | second start reports running | two daemons on one store → refused |
| TEST-102 | REQ-102 | CLI routes via live socket | garbage frame → error+close, daemon lives | silent fallback while daemon lives → fail |
| TEST-103 | REQ-103 | invite prints code once; record published | ttl=0 rejected | code in logs/json → fail |
| TEST-104 | REQ-104 | two-process localhost join end-to-end | wrong words → auth-failed, no partial state | joiner replica ≠ inviter corpus → fail |
| TEST-105 | REQ-105 | member fact appears, closure-derived roster includes joiner | — | joiner absent from roster → fail |
| TEST-106 | REQ-106 | republish before expiry (mock clock) | — | stale endpoint never refreshed → fail |
| TEST-107 | REQ-107 | member connects | non-member NodeId refused pre-frame | non-member got sync bytes → fail |
| TEST-108 | REQ-108 | 100-entry diff converges; equal vv after | — | vv unequal after session → fail |
| TEST-109 | REQ-109 | remote merge fires local watch push | — | push without tag change → fail |
| TEST-110 | REQ-110 | second join on used code fails | expired code fails | rendezvous record survives consumption → fail |
| TEST-111 | REQ-111 | wrong-word remote sees only auth-failed | — | error string leaks reason remotely → fail |
| TEST-112 | NFR-101/102/103 | timed harness, localhost + netem | — | budgets exceeded → fail |
| TEST-113 | CON-101/102 | fuzz: control frames, invite codes | crash/hang → fail | — |
| TEST-114 | CON-103/REQ-021 | property: random partitions/reorders/dupes ⇒ convergence | — | divergent closures on equal corpora → fail |

Verification techniques: fuzzing REQUIRED at all three network-facing
recognisers (control frames, invite codes, sync frames feeding Loro
import); property-based convergence testing over simulated partitions;
the pairing ceremony is Tier-1 — cross-model adversarial review plus
human crypto sign-off before any release claims adversarial soundness
(inherited open item from [[SPEC-047]]).

## 5. Observability

OBS-101: daemon tracing spans per connection/session with theory id,
peer NodeId, frames in/out, entries merged/quarantined. OBS-102:
`daemon status --json` counters (uptime, per-theory peers, last sync,
quarantine count, closure p95). OBS-103: join ceremony emits a
structured audit line (invite id, outcome, no secrets).

## Changelog

<details>
<summary>Revision history</summary>

- 0.1.0 — implemented: daemon (loopback API, flock lifecycle, push watch), SPAKE2 join ceremony (proven over duplex), iroh QUIC transport + Mainline-DHT discovery wired (live loopback test ignored).

- 0.1.0 — initial specification, patterned on [[SPEC-047]] with
  elephant-specific membership (corpus-evidence roster) and the
  local-JSON control plane decision (ADR-106).
</details>
