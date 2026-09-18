---
title: Elephant project status
mode: explanation
---

# Elephant project status

Elephant remains in the `v0.1` series.
[[CHANGELOG]] distinguishes tagged releases from unreleased changes.
The core implements identity, journals, reasoning, commitments, and queries.
The [[Daemon]] supplies a loopback control API.
[[MLS]] encryption, member removal with key rotation, and the [[SPAKE2]] join ceremony are implemented.

## Sync and its verification boundary

Continuous peer sync is implemented and opt-in through `ELEPHANT_SYNC_INTERVAL`.
The daemon accepts roster members and periodically dials known peers to exchange [[Loro]] deltas.
The obligations are [[SPEC-002-elephant-p2p#REQ-107]] and [[SPEC-002-elephant-p2p#REQ-108]].

Unit and in-memory duplex tests cover the dialect, roster gate, sync session, and grow-only import.
Join choreography and removal tests cover the composed membership operations.
Live iroh QUIC discovery tests remain ignored in the default suite because they depend on endpoint discovery.
Passing local tests therefore does not establish live-network availability.

## Open boundaries

The local member can decrypt its stored corpus; storage-level adversaries are outside the documented model.
The SPAKE2, MLS, and keybook composition retains an open Tier-1 human cryptographic review.
[[SPEC-004-elephant-e2ee]] records that review obligation, inherited from [[SPEC-047]].
The open item remains separate from the automated join and removal tests.

Agent orchestration and plan translation remain deferred from [[hence]].
[[architecture]] explains the current coordination boundary.

## Background

The design draws on these sources:

- [[Elephant 2000]] — McCarthy's 1989 model of programs based on speech acts.
- [[Defeasible Logic]] — the Nute and SPINdle reasoning lineage.
- [[CBCL]] — the speech-act communication language used by Elephant.
- [[MLS]] — group encryption, specified by RFC 9420.
- [[SPAKE2]] — password-authenticated exchange, specified by RFC 9382.
- [[Mainline DHT]] — mutable discovery records through BEP-44.
