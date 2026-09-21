---
title: Elephant project status
mode: explanation
---

# Elephant project status

Elephant remains in the `v0.1` series.
[CHANGELOG](../../CHANGELOG.md) distinguishes tagged releases from unreleased changes.
The core implements identity, journals, reasoning, commitments, and queries.
The [Daemon](../concepts/Daemon.md) supplies a loopback control API.
[MLS](../concepts/MLS.md) encryption, member removal with key rotation, and the [SPAKE2](../concepts/SPAKE2.md) join ceremony are implemented.

## Sync and its verification boundary

Continuous peer sync is implemented and opt-in through `ELEPHANT_SYNC_INTERVAL`.
The daemon accepts roster members and periodically dials known peers to exchange [Loro](../concepts/Loro.md) deltas.
The obligations are [REQ-107](../../specs/SPEC-002-elephant-p2p.md#user-content-req-107-roster-gate-implemented) and [REQ-108](../../specs/SPEC-002-elephant-p2p.md#user-content-req-108-corpus-sync).

Unit and in-memory duplex tests cover the dialect, roster gate, sync session, and grow-only import.
Join choreography and removal tests cover the composed membership operations.
Live iroh QUIC discovery tests remain ignored in the default suite because they depend on endpoint discovery.
Passing local tests therefore does not establish live-network availability.

## Open boundaries

The local member can decrypt its stored corpus; storage-level adversaries are outside the documented model.
The SPAKE2, MLS, and keybook composition retains an open Tier-1 human cryptographic review.
[SPEC-004-elephant-e2ee](../../specs/SPEC-004-elephant-e2ee.md) records that review obligation, inherited from [SPEC-047](../concepts/SPEC-047.md).
The open item remains separate from the automated join and removal tests.

Agent orchestration and plan translation remain deferred from [hence](../concepts/hence.md).
[Elephant architecture](architecture.md) explains the current coordination boundary.

## Background

The design draws on these sources:

- [Elephant 2000](../concepts/Elephant%202000.md) — McCarthy's 1989 model of programs based on speech acts.
- [Defeasible Logic](../concepts/Defeasible%20Logic.md) — the Nute and SPINdle reasoning lineage.
- [CBCL](../concepts/CBCL.md) — the speech-act communication language used by Elephant.
- [MLS](../concepts/MLS.md) — group encryption, specified by RFC 9420.
- [SPAKE2](../concepts/SPAKE2.md) — password-authenticated exchange, specified by RFC 9382.
- [Mainline DHT](../concepts/Mainline%20DHT.md) — mutable discovery records through BEP-44.
