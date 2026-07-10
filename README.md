<h1 align="center">elephant</h1>

<p align="center">
  <strong>Speech-act coordination on shared, end-to-end-encrypted defeasible theories.</strong><br>
  <em>John McCarthy's Elephant 2000, made computable.</em>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
  <img alt="Stage: v0.1" src="https://img.shields.io/badge/stage-v0.1-orange">
</p>

> "I meant what I said, and I said what I meant.
> An elephant's faithful, one hundred percent!
> moreover, an elephant never forgets."

## What this is

elephant is a CLI + daemon where agents — humans, LLMs, CI bots, webhooks —
coordinate by exchanging **signed speech acts** into shared, append-only
**logical theories**. Whether a task is done, a promise is kept, or a claim
stands is *derived* by defeasible reasoning over signed evidence — never
decreed by a status column.

It is the successor to [hence](https://codeberg.org/anuna/hence): the same
task-coordination surface (`plan board`, `task claim/complete/block`,
`query why-not/require/what-if`), but the plan is no longer a local file —
it is a peer-to-peer, encrypted theory that travels.

The concept comes from three observations:

- **Coordination is stigmergic.** A Jira column's "one assignee" leaks the
  moment a linter, a reviewer, an LLM, a webhook, and a human all have
  opinions on one ticket. Completion should be a *conclusion*, not a
  transition.
- **Defeasible logic already handles mixed-trust, mixed-authority claims** —
  facts, rules, defeaters, preferences, trust weights. elephant embeds
  [spindle-rust](https://codeberg.org/anuna/spindle-rust) for the closure.
- **McCarthy already designed the semantics.** *Elephant 2000* (1989):
  programs whose I/O is speech acts, that refer directly to the past instead
  of data structures, and whose correctness is "did it keep its promises?".
  elephant makes a promise a first-class object whose fulfilment or
  violation is *derived* from the corpus.

## Quick start

```bash
make build                                  # single binary: target/release/elephant
make install                                # → ~/.local/bin/elephant

elephant id create --name alice             # Ed25519 key + did:crdt identity
elephant theory create release              # a new encrypted theory

elephant assert 'qa-signed' -t release
elephant assert '(normally r-ready (and qa-signed legal-signed) release-ready)' -t release
elephant status -t release                  # release-ready: -d  (legal-signed missing)
elephant why-not release-ready -t release   # rule r-ready: missing legal-signed
elephant assert 'legal-signed' -t release
elephant status -t release                  # release-ready: +d
```

Promises (the Elephant 2000 core):

```bash
elephant promise 'legal-signed' --by 2026-07-18T17:00:00Z -t release
elephant commitments -t release             # legal-signed … outstanding
elephant assert 'legal-signed' -t release
elephant commitments -t release             # legal-signed … fulfilled
```

## Usage

### Joining across machines

Alice invites; Bob joins with a spoken code. No server, no key exchange:

```bash
# Alice
elephant theory invite release
#   Share this one-time code:  7842-walnut-harbor

# Bob (code entered on the TTY, never argv/logs)
elephant theory join 7842-walnut-harbor --alias release
```

Under the hood: the number routes (it derives a
[pkarr](https://pkarr.org) rendezvous record on the BitTorrent Mainline
DHT); the two words are the [SPAKE2](https://datatracker.ietf.org/doc/html/rfc9382)
password (never on the wire). After key confirmation, Alice adds Bob to the
theory's MLS group, seals the introduction, hands over the keybook, and Bob
pulls the whole (encrypted) history. A wrong code fails opaquely and leaves
no partial state.

### The task layer (hence-compatible)

```bash
elephant theory create sprint --template plan
elephant plan join-as alice -t sprint
elephant task next --agent alice -t sprint
elephant task claim models -t sprint
elephant task complete models -t sprint      # unblocks dependents
elephant plan board -t sprint
```

The lifecycle SPL is byte-compatible with hence 0.7's chain-cancellation
bundles, so existing hence plans and agent scripts port directly.

### The daemon

```bash
elephant daemon start                        # holds replicas, syncs continuously
elephant status -t release                   # reflects peers' assertions
elephant watch release-ready -t release      # pushed on tag change
elephant daemon stop
```

One-shot commands route through the daemon when it is live (single writer)
and fall back to direct store access when it is not. One identity
participates in any number of fully-disjoint theories.

## Architecture

Four sibling libraries do the load-bearing work; elephant is the glue.

```
       ┌────────────── elephant ───────────────────┐
       │  CLI (clap)          daemon (loopback API) │
       │      │                   │                 │
       │      ▼                   ▼                 │
       │  ┌───────────────────────────────────┐     │
       │  │ theory store — one LoroDoc/theory │     │
       │  │ sealed append-only corpus (MLS)   │     │
       │  └───────────────┬───────────────────┘     │
       │                  │ verify → open → close    │
       │  ┌───────────────▼───────────────────┐     │
       │  │ PURE CORE  closure(corpus,trust,t)│     │
       │  │ envelope · tombstone · commitments│     │
       │  └──┬────────┬────────┬────────┬─────┘     │
       └─────┼────────┼────────┼────────┼───────────┘
        ┌────▼──┐ ┌───▼────┐ ┌─▼──────┐ ┌▼───────┐
        │cbcl-rs│ │spindle-│ │did-crdt│ │ openmls│
        │dialect│ │rust SPL│ │identity│ │  MLS   │
        └───────┘ └────────┘ └────────┘ └────────┘
   transport: iroh QUIC + pkarr/Mainline-DHT · join: SPAKE2
```

- **[cbcl-rs](https://codeberg.org/anuna/cbcl-rs)** — the `cbcl-elephant`
  speech-act dialect (assert, retract, query, concede, commit, request,
  justify), canonical bytes, Lean-verified R1–R4 invariants.
- **[spindle-rust](https://codeberg.org/anuna/spindle-rust)** — defeasible
  closure, trust weighting, explain/why-not/require/what-if.
- **[did-crdt](https://github.com/anuna-research/did-crdt)** — `did:crdt`
  identity (pure core, no networking).
- **[loro](https://loro.dev)** — the replicated append-only corpus.
- **[openmls](https://openmls.tech)** — one MLS group per theory; corpus
  entries sealed under keybook data keys; removal rotates the key.

The **pure core** (`src/core/*`) is a deterministic function of its
arguments — closure takes the evaluation time as a parameter, never a
syscall — which is what makes convergence (identical conclusions on every
replica, in any merge order) a property test rather than a hope. See the
specs for the full design:

- [`specs/SPEC-001-elephant-core.md`](specs/SPEC-001-elephant-core.md) — corpus, closure, commitments.
- [`specs/SPEC-002-elephant-p2p.md`](specs/SPEC-002-elephant-p2p.md) — daemon, SPAKE2 join, discovery, sync.
- [`specs/SPEC-003-elephant-tasks.md`](specs/SPEC-003-elephant-tasks.md) — the hence-successor task layer.
- [`specs/SPEC-004-elephant-e2ee.md`](specs/SPEC-004-elephant-e2ee.md) — MLS end-to-end encryption.

## Development

```bash
make check            # cargo test + fmt --check + clippy -D warnings
make test             # cargo test
make bench            # criterion NFR benchmarks (closure latency)
cargo +nightly fuzz run invite_code   # any of: entry_json spl_payload cbcl_wire invite_code sealed_entry
zetl check --fail-on error            # spec-vault traceability (this repo is a zetl vault)
```

Requires a Rust toolchain ≥ 1.85 (spindle-core is edition 2024). The four
Anuna sibling repos are consumed as path dependencies (`../cbcl-rs`,
`../spindle-rust`, `../did-crdt`); clone them alongside this one.

## Status

**v0.1.** The core (identity, corpus, closure, commitments, queries), the
hence-successor task layer, the daemon, MLS end-to-end encryption, and the
SPAKE2 join ceremony are implemented and tested (83 tests; the join
choreography and the E2EE removal property are covered end-to-end). The
live iroh-QUIC transport is wired but its loopback test is `#[ignore]`d
(needs endpoint discovery to settle); the ceremony itself is proven over an
in-memory duplex.

**Not yet:** membership revocation (the roster is grow-only), corpus
encryption is symmetric-at-rest per member (no storage-level adversary
model), and the SPAKE2∘MLS∘keybook composition carries an inherited Tier-1
crypto-review open item from [SPEC-047](specs/SPEC-004-elephant-e2ee.md).
LLM-orchestration features from hence (`agent spawn/watch`, `plan
translate/decompose`) are deliberately deferred. See each spec's
Orientation → Open section.

## References

- McCarthy, J. *Elephant 2000: A Programming Language Based on Speech Acts.*
  Stanford, 1989. <https://www-formal.stanford.edu/jmc/elephant.pdf>
- Nute, D. *Defeasible Logic.* (the SPINdle lineage spindle-rust implements)
- O'Connor, H. *CBCL: Safe Self-Extending Agent Communication.* LangSec 2026.
- RFC 9420 (MLS), RFC 9382 (SPAKE2), BEP-44 (Mainline DHT mutable items).

## License

Apache-2.0.
