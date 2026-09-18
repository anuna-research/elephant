---
title: Elephant architecture
mode: explanation
---

# Elephant architecture

Elephant coordinates participants through signed [[Speech Act|speech acts]] and shared [[Logical Theory|logical theories]].
It continues [[hence]]'s use of [[Defeasible Logic]] while replacing plan files with encrypted peer replicas.

A shared conclusion can reflect evidence from humans, agents, and CI systems at once.
Rules express how those claims combine or conflict.
[[Commitment|Commitments]] make responsibility explicit, and the theory derives their fulfilment from evidence.
This follows [[Elephant 2000]]'s model of programs that communicate through speech acts and refer to earlier statements.

## Boundaries

The CLI and [[Daemon]] form the effectful shell around the pure reasoning core:

```text
CLI / loopback API
        |
        v
signed entries -> theory store <-> encrypted peer sync
                       |
                verify / decrypt
                       |
                       v
          pure core(corpus, trust, time)
                       |
                       v
          conclusions / commitments
```

The shell owns identities, storage, transport, and clocks.
The core takes evaluation time as an argument; it does not read the clock itself.
Deterministic inputs allow convergence checks across replicas and journal merge orders.
The closure contract is [[SPEC-001-elephant-core#CON-003]].
[[SPEC-001-elephant-core#5. Purity Boundary Map]] defines the core, shell, and their dependency boundary.

## Composition

The implementation composes existing components rather than introducing a separate reasoning engine or identity network:

- `cbcl-rs` supplies the [[cbcl-elephant]] dialect, canonical bytes, and its R1–R4 invariants.
- `spindle-rust` supplies [[SPL]], defeasible closure, trust reasoning, and explanatory queries.
- `did-crdt` supplies [[DID]] identity without adding another networking stack.
- [[Loro]] replicates the append-only [[Corpus]].
- OpenMLS supplies the [[MLS]] group and key rotation used by each theory.

The relevant choices are [[SPEC-001-elephant-core#ADR-001]], [[SPEC-001-elephant-core#ADR-002]], and [[SPEC-001-elephant-core#ADR-003]].
The Anuna libraries are sibling path dependencies; Loro and OpenMLS are external dependencies declared in `Cargo.toml`.

## Sharing

[[Theory Join]] uses [[SPAKE2]] to authenticate an invitation before transferring group membership and encrypted history.
The invite number locates the rendezvous through [[pkarr]] and [[Mainline DHT]].
The words supply the password for the exchange.
Steady-state sync uses iroh QUIC and the roster-gated `cbcl-elephant-sync` dialect.

The daemon serves local commands and continuous peer sync when enabled.
Without a live daemon, supported one-shot commands access the local store directly.
That direct path does not provide continuous peer replication.

The design details live in these specifications:

- [[SPEC-001-elephant-core]] — corpus, closure, and commitments.
- [[SPEC-002-elephant-p2p]] — daemon, discovery, joining, and sync.
- [[SPEC-003-elephant-tasks]] — retained lifecycle vocabulary and retired task-writing commands.
- [[SPEC-004-elephant-e2ee]] — encryption and member removal.
- [[SPEC-005-elephant-vocabulary]] — predicate documentation and near-miss advice.
- [[SPEC-006-elephant-next]] — theory-derived work discovery.

## Scope

Elephant exposes speech acts and queries rather than an agent supervisor.
It does not provide `agent spawn/watch` or `plan translate/decompose`.
Local execution tools can supply worktrees and process management while Elephant records shared responsibility and evidence.
A process exit alone does not establish acceptance of a task.

[[project-status]] records the current verification limits and open cryptographic review.
