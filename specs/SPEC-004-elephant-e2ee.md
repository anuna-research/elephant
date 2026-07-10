---
id: SPEC-004
title: elephant e2ee — MLS-based end-to-end encryption of theory corpora
version: 0.1.0
status: approved
date: 2026-07-11
last-updated: 2026-07-11
audience: agent, human reviewer
---

# SPEC-004 — elephant e2ee

## Orientation

Intent: Every theory's corpus is end-to-end encrypted: only current
members (and, for history, members admitted later — by explicit design)
can read it. Group key agreement uses [[MLS]] (RFC 9420); a removed
member cannot read anything written after their removal.

Metaphor: the minute-book gets a lockable cover — MLS manages who holds
the current key; the keybook inside the cover lets a newly sworn-in
member read the whole history, because a theory that forgets its past is
no elephant.

Structure:

```
  identity key ──HKDF──▶ MLS leaf signing key (CON-304)
                            │
        ┌── MLS group per theory (group_id = theory id) ──┐
        │  credential = DID · steward commits only        │
        │  commits ride the doc's "mls" lane (CON-302)    │
        │  Welcome over the SPAKE2 join channel           │
        └───────────────┬──────────────────────────────────┘
                        │ application messages
                        ▼
                keybook {gen_i → K_i}  (CON-303)
                        │
                        ▼
   Entry ──seal K_gen──▶ SealedEntry {v,gen,nonce,ct} (CON-301)
                        stored in the Loro corpus list
```

Decisions: [[#ADR-301]] openmls 0.8 · [[#ADR-302]] random data keys +
keybook, not per-epoch exporters · [[#ADR-303]] steward-only commits ·
[[#ADR-304]] leaf key derived from identity seed.

Load-bearing: [[#REQ-303]] sealing · [[#REQ-304]] keybook ·
[[#REQ-305]] removal → rotation · [[#REQ-306]] commit ordering.

Open: Tier-1 human crypto review of the whole join+E2EE composition
(SPAKE2 ∘ MLS ∘ keybook) before any adversarial-soundness claim (owner
HOC) · fork tolerance beyond steward policy (dMLS / openmls
fork-resolution, [[#ADR-303]] ceiling).

Detail: [[SPEC-001-elephant-core]] (Entry), [[SPEC-002-elephant-p2p]]
(join ceremony, roster gate, sync).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD
NOT, RECOMMENDED, MAY, and OPTIONAL in this document are to be
interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only
when, they appear in all capitals.

## 1. Requirements

#### REQ-301: MLS group per theory

`elephant theory create` SHALL create an MLS group whose `group_id` is
the theory id, ciphersuite
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`, with the creator as sole
initial member and steward. All MLS state SHALL survive daemon/CLI
restarts ([[#CON-305]]).

Trace: [[#TEST-301]]

#### REQ-302: DID-bound credentials

Each member's MLS leaf SHALL use a basic credential whose identity octets
are the member's [[DID]] string, and membership admission SHALL verify
that the credential's DID matches the DID authenticated during the join
ceremony. The MLS leaf signature key is derived per [[#ADR-304]].

Trace: [[#TEST-302]]

#### REQ-303: Corpus sealing

Every Entry SHALL be stored and synced only as a SealedEntry
([[#CON-301]]): AEAD ciphertext under the theory's current data key
`K_gen`. Plaintext Entries SHALL never be written to the Loro doc, disk,
or wire. Decryption failures at merge SHALL quarantine the element
(fail closed, [[SPEC-001-elephant-core#REQ-022]] extended).

Trace: [[#TEST-303]] · [[#TEST-306]] (fuzz)

#### REQ-304: Keybook — history for new members

The theory's data keys SHALL be recorded in a keybook
`{gen → K_gen}` ([[#CON-303]]). After adding a member, the steward SHALL
send the complete keybook as an MLS application message in the new
epoch, so the joiner can read the entire corpus history. This
history-visibility policy is deliberate and per-theory-fixed in v0.1.

Trace: [[#TEST-304]]

#### REQ-305: Removal and rotation

`elephant theory remove <did>` (steward only) SHALL: MLS-remove the
member (commit → new epoch), generate a fresh `K_{gen+1}`, distribute
the updated keybook via an MLS application message in the new epoch,
and seal all subsequent Entries under `K_{gen+1}`. The removed member
SHALL be unable to decrypt any Entry sealed after rotation, and SHALL be
refused at the transport roster gate ([[SPEC-002-elephant-p2p#REQ-107]]).
A membership-retraction fact SHALL be asserted so closures reflect the
removal.

Trace: [[#TEST-305]]

#### REQ-306: Commit ordering

Only the steward SHALL issue MLS commits in v0.1 ([[#ADR-303]]). Commits
SHALL be appended to the theory doc's `mls` lane ([[#CON-302]]) and
members SHALL process them strictly in epoch order, buffering
future-epoch commits and never skipping. A non-steward commit SHALL be
rejected and logged.

Trace: [[#TEST-307]]

#### REQ-307: MLS state persistence

MLS group state, keybook, and pending invites SHALL persist atomically
under the theory's state dir ([[#CON-305]]), files 0600; a crash between
processing a commit and persisting SHALL NOT strand the member (process
+ persist are one transaction from the caller's view).

Trace: [[#TEST-308]]

#### NFR-301: Seal/open overhead ≤ 15 % over plaintext corpus operations
on the [[SPEC-001-elephant-core#NFR-001]] benchmark theory.

#### NFR-302: Rotation-to-first-sealed-entry ≤ 2 s locally, 95p.

#### NFR-303: The keybook and all MLS secrets SHALL never appear in
logs, JSON output, or error messages.

## 2. Architecture decisions

#### ADR-301: openmls 0.8

**Decision.** Use `openmls 0.8` + `openmls_rust_crypto` +
`openmls_basic_credential`, with a durable provider modelled on hark's
(`../hark/src/mls/provider.rs`: upstream MemoryStorage + atomic
whole-snapshot persist to a 0600 file).

**Context.** Two mature options exist. mls-rs (AWS) offers 100 % RFC
9420 conformance, `out_of_order`/`prior_epoch` transport tolerance and a
cleaner 3-trait storage surface; openmls offers a sync API, the
ecosystem's first fork-resolution helpers, and — decisively — an
existing in-house integration in `../hark` (provider, pins, spike)
that this repo can copy (Simplicity Ladder rung 4). Under
[[#ADR-303]]'s single-committer policy the transport-tolerance
advantages of mls-rs are not load-bearing: application messages are
rare (keybook distribution) and steward-ordered.

**Trade-offs.** (+) proven in-house pattern; sync API matches the core;
fork-resolution path exists if ADR-303's ceiling is hit. (−) basic
credentials only (sufficient — DID in the credential, [[#REQ-302]]);
large storage trait (mitigated by snapshot provider).

#### ADR-302: Random data keys + keybook, not per-epoch exporter keys

**Decision.** Corpus encryption keys are random 32-byte keys `K_gen`,
generated at theory creation and at each rotation, distributed inside
MLS application messages (keybook). MLS exporter secrets are NOT used as
data keys.

**Context.** MLS forward secrecy deletes old epochs' secrets — by
design a new member can derive nothing from before their join. But an
elephant theory's history MUST be readable by admitted members
([[Elephant 2000]] thesis 3; the corpus *is* the state). The research
survey confirms the standard resolution: content keys distributed under
MLS protection, with the full key history handed to joiners. Rotation
on removal preserves the property that matters: post-removal
confidentiality ([[#REQ-305]]). Exporters remain available for future
per-purpose keys (e.g. at-rest DB encryption) with distinct labels.

**Trade-offs.** (+) history readable; removal cheap (no corpus
re-encryption); keybook is tiny. (−) compromise of any current member
reveals all history — identical to the plaintext-visibility any member
already has; documented, not hidden.

#### ADR-303: Steward-only commits

**Decision.** The theory creator is the steward: the only member who
may invite (Add), remove, or otherwise commit. MLS forks are thereby
prevented rather than resolved.

**Trade-offs.** (+) no delivery-service problem; deterministic epochs
on a gossiped lane. (−) invites require the steward online (already
true of the SPAKE2 ceremony, [[SPEC-002-elephant-p2p#ADR-102]]);
steward loss freezes membership (corpus keeps working — reads, writes
and sync among existing members are unaffected).
// SIMPLIFY: single steward; ceiling: steward unavailability blocking
// membership change in practice; upgrade path: proposal queue +
// designated-committer rotation, or dMLS/openmls fork-resolution
// (trace: this ADR, [[SPEC-002-elephant-p2p#ADR-104]]).

#### ADR-304: MLS leaf key derived from the identity seed

**Decision.** The MLS leaf Ed25519 signing key is
`HKDF-SHA256(ikm = identity seed, info = "elephant/mls-leaf/v1" ‖ theory-id)`
— per-theory distinct, never the identity key itself (no cross-protocol
key reuse), and recoverable from the single identity backup.

## 3. Contracts

#### CON-301: SealedEntry

```rust
struct SealedEntry {
  v: u16,          // = 1
  gen: u32,        // keybook generation of the sealing key
  nonce: [u8; 24], // XChaCha20-Poly1305, random per entry
  ct: Vec<u8>,     // AEAD over canonical Entry JSON (SPEC-001 CON-002)
}                  // AAD = "elephant-seal-v1" ‖ theory-id ‖ gen
```

Canonical JSON, `deny_unknown_fields`, element ≤ 80 KiB. This is what
the Loro corpus list stores; [[SPEC-001-elephant-core#CON-002]]'s Entry
is the plaintext inside `ct`. Grammar: JSON → serde closed type; full
recognition (and AEAD verification) before any semantic action.
Implements: [[#REQ-303]]. Verified by: [[#TEST-303]] [[#TEST-306]].

#### CON-302: The `mls` lane

Second Loro list container `"mls"` in each theory doc; elements
`{v:1, epoch: u64, kind: "commit", mls: <TLS-serialized MLSMessage,
base64>}` appended by the steward only. Members process in ascending
epoch order; future epochs buffer; duplicates idempotent. Welcome
messages never ride this lane (they go over the SPAKE2 join channel);
application messages (keybook) DO ride it — they are readable only by
members at that epoch, which is exactly the intent.
Implements: [[#REQ-306]] [[#REQ-304]]. Verified by: [[#TEST-307]].

#### CON-303: Keybook message

MLS application message, canonical JSON
`{v:1, keys: {"0": b64(K_0), "1": b64(K_1), …}, current: <gen>}`.
Sent by the steward after every Add and every rotation. Receivers
merge by union and verify `current` monotonicity.
Implements: [[#REQ-304]] [[#REQ-305]]. Verified by: [[#TEST-304]]
[[#TEST-305]].

#### CON-304: Key derivation

`leaf_seed = HKDF-SHA256(ikm=identity_seed, salt="elephant/v1",
info="mls-leaf" ‖ theory_id, L=32)`. Rendezvous and discovery keys
([[SPEC-002-elephant-p2p#CON-104]]) use the same HKDF with their own
info labels; all domain strings are compile-time constants in one
module, KAT-pinned.
Implements: [[#REQ-302]] · [[SPEC-002-elephant-p2p#CON-102]].

#### CON-305: MLS state at rest

`theories/<id>/mls/{state.bin, keybook.json}` — 0600, written via
tmp-file + rename after every MLS operation (hark provider pattern);
`state.bin` is the serialized openmls storage snapshot. Loading an
incompatible snapshot version fails closed with a rejoin instruction,
never silent reset.
Implements: [[#REQ-307]]. Verified by: [[#TEST-308]].

## 4. Test specification

| TEST | Validates | Positive | Negative-input | Negative-output |
|---|---|---|---|---|
| TEST-301 | REQ-301 | create → group exists, epoch 0, steward=creator | duplicate group id → refused | — |
| TEST-302 | REQ-302 | credential DID = join DID | mismatched DID at admission → refused | — |
| TEST-303 | REQ-303 | seal→open roundtrip; corpus file contains no plaintext markers | tampered ct/nonce/gen → quarantine | plaintext Entry found in doc bytes → fail |
| TEST-304 | REQ-304 | joiner reads full pre-join history | keybook with regressed `current` → rejected | joiner missing old gen → fail |
| TEST-305 | REQ-305 | post-rotation entries unreadable with pre-rotation keys | non-steward remove → refused | removed member decrypts new entry → fail |
| TEST-306 | CON-301 | fuzz SealedEntry decode+open | crash/hang → fail | — |
| TEST-307 | REQ-306 | in-order commit processing; future-epoch buffering | non-steward commit → rejected+logged | skipped epoch accepted → fail |
| TEST-308 | REQ-307 | kill-restart mid-sequence → state consistent | corrupted state.bin → fail-closed message | silent state reset → fail |

Tier-1: this spec is a no-go-area artefact (cryptography). Mandatory
before release claims: cross-model adversarial review of §2–§3 and human
crypto review of the SPAKE2 ∘ MLS ∘ keybook composition.

## Changelog

<details>
<summary>Revision history</summary>

- 0.1.0 — initial, added on stakeholder directive ("implement message
  layer security for E2EE"); supersedes SPEC-002 ADR-105's deferral.
</details>
