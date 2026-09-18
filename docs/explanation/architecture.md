---
title: Elephant architecture
mode: explanation
---

# Elephant architecture

Elephant coordinates participants through signed [speech acts](../concepts/Speech%20Act.md) and shared [logical theories](../concepts/Logical%20Theory.md).
It continues [hence](../concepts/hence.md)'s use of [Defeasible Logic](../concepts/Defeasible%20Logic.md) while replacing plan files with encrypted peer replicas.

A shared conclusion can reflect evidence from humans, agents, and CI systems at once.
Rules express how those claims combine or conflict.
[Commitments](../concepts/Commitment.md) make responsibility explicit, and the theory derives their fulfilment from evidence.
This follows [Elephant 2000](../concepts/Elephant%202000.md)'s model of programs that communicate through speech acts and refer to earlier statements.

## Boundaries

The CLI and [Daemon](../concepts/Daemon.md) form the effectful shell around the pure reasoning core:

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
The closure contract is [CON-003](../../specs/SPEC-001-elephant-core.md#user-content-con-003-closure-pipeline-pure-core).
[5. Purity Boundary Map](../../specs/SPEC-001-elephant-core.md#user-content-5-purity-boundary-map) defines the core, shell, and their dependency boundary.

## Composition

The implementation composes existing components rather than introducing a separate reasoning engine or identity network:

- `cbcl-rs` supplies the [cbcl-elephant](../concepts/cbcl-elephant.md) dialect, canonical bytes, and its R1–R4 invariants.
- `spindle-rust` supplies [SPL](../concepts/SPL.md), defeasible closure, trust reasoning, and explanatory queries.
- `did-crdt` supplies [DID](../concepts/DID.md) identity without adding another networking stack.
- [Loro](../concepts/Loro.md) replicates the append-only [Corpus](../concepts/Corpus.md).
- OpenMLS supplies the [MLS](../concepts/MLS.md) group and key rotation used by each theory.

The relevant choices are [ADR-001](../../specs/SPEC-001-elephant-core.md#user-content-adr-001-reuse-the-shipped-cbcl-elephant-dialect-verbatim), [ADR-002](../../specs/SPEC-001-elephant-core.md#user-content-adr-002-corpus-one-lorodoc-per-theory-append-only-entry-list), and [ADR-003](../../specs/SPEC-001-elephant-core.md#user-content-adr-003-identity-did-crdt-core-key-custody-is-elephant-s).
The Anuna libraries are sibling path dependencies; Loro and OpenMLS are external dependencies declared in `Cargo.toml`.

## Sharing

[Theory Join](../concepts/Theory%20Join.md) uses [SPAKE2](../concepts/SPAKE2.md) to authenticate an invitation before transferring group membership and encrypted history.
The invite number locates the rendezvous through [pkarr](../concepts/pkarr.md) and [Mainline DHT](../concepts/Mainline%20DHT.md).
The words supply the password for the exchange.
Steady-state sync uses iroh QUIC and the roster-gated `cbcl-elephant-sync` dialect.

The daemon serves local commands and continuous peer sync when enabled.
Without a live daemon, supported one-shot commands access the local store directly.
That direct path does not provide continuous peer replication.

The design details live in these specifications:

- [SPEC-001-elephant-core](../../specs/SPEC-001-elephant-core.md) — corpus, closure, and commitments.
- [SPEC-002-elephant-p2p](../../specs/SPEC-002-elephant-p2p.md) — daemon, discovery, joining, and sync.
- [SPEC-003-elephant-tasks](../../specs/SPEC-003-elephant-tasks.md) — retained lifecycle vocabulary and retired task-writing commands.
- [SPEC-004-elephant-e2ee](../../specs/SPEC-004-elephant-e2ee.md) — encryption and member removal.
- [SPEC-005-elephant-vocabulary](../../specs/SPEC-005-elephant-vocabulary.md) — predicate documentation and near-miss advice.
- [SPEC-006-elephant-next](../../specs/SPEC-006-elephant-next.md) — theory-derived work discovery.

## Scope

Elephant exposes speech acts and queries rather than an agent supervisor.
It does not provide `agent spawn/watch` or `plan translate/decompose`.
Local execution tools can supply worktrees and process management while Elephant records shared responsibility and evidence.
A process exit alone does not establish acceptance of a task.

[Elephant project status](project-status.md) records the current verification limits and open cryptographic review.
