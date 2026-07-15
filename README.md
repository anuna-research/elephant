<h1 align="center">elephant</h1>

<p align="center">
  <strong>Speech-act coordination on shared, end-to-end-encrypted defeasible theories.</strong>
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

It is the successor to [hence](https://codeberg.org/anuna/hence) — in
ideas, not surface. hence coordinated work by treating completion as a
defeasible conclusion over a local plan file; elephant carries that idea
onto a peer-to-peer, encrypted theory that travels, and drops the
file-bound task-verb surface. Whether a task is done is still something
you *ask the theory* (`status`, `why-not`, `require`, `what-if`), not a
column you drag.

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

Vocabulary (SPEC-005) — the theory's working glossary, derived from the
corpus, never a pinned schema:

```bash
# document a predicate before (or after) anyone asserts it — validated,
# signed, and synced like any other statement
elephant define ci-green/1 --arg task:symbol \
    --desc "CI pipeline green for task ?t" --kind evidence --asserter role:ci -t release

# a rule that listens for (ci-green ?t) makes it a live part of the vocabulary
elephant assert '(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))' -t release

elephant vocab -t release                   # families × roles × class × docs
#   active  predicate  ci-green/1         body  — CI pipeline green for task ?t
#   active  predicate  review-approved/1  body
#   active  predicate  verified/1         head  — evidence a task's work is verified …  [built-in]

# prove one witness so (ci-green m1) is demanded but still unproven …
elephant assert '(given (review-approved m1))' -t release

# … then why-not / require answers carry the documentation of whatever is
# missing, and a daemon-served assert that lands on a near-miss sibling gets
# a near-miss advisory in its receipt (never an error):
elephant assert '(given (ci-green m2))' -t release
#   advisory (sibling): family ci-green/1
#     did you mean (ci-green m1)?  (listener r-verified)
```

> The advisory is best-effort and daemon-only: it is read from the daemon's
> cached view and is silently absent on a direct-store assert, or on the
> *first* assert to a theory after the daemon (re)starts (its view is not yet
> warm) — never a blocked or failed assert (SPEC-005 NFR-402).

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

### The daemon

```bash
export ELEPHANT_SYNC_INTERVAL=30             # opt in to continuous P2P sync (seconds)
elephant daemon start                        # single writer + change watch + sync
elephant status -t release                   # reflects peers' assertions
elephant watch release-ready -t release      # pushed on tag change
elephant daemon stop
```

The daemon is the local single writer and change-notification plane. With
`ELEPHANT_SYNC_INTERVAL` set, it also runs continuous P2P sync: it serves
incoming sync sessions (roster-gated) and dials each theory's roster peers on
that interval, so members reconverge without an operator. The sync wire is the
`cbcl-elephant-sync` dialect (typed `offer`/`deliver` speech acts over iroh
QUIC). Unset or `0` leaves sync off (see **Status** for why it is opt-in).

One-shot commands route through the daemon when it is live (single writer)
and fall back to direct store access when it is not. One identity
participates in any number of fully-disjoint theories.

#### Running under launchd (macOS)

To keep the daemon alive across logins and crashes, run it as a per-user
LaunchAgent. Use `daemon run` (foreground) — launchd supervises the process
itself, so the self-backgrounding `daemon start` would confuse it. Save as
`~/Library/LaunchAgents/io.anuna.elephant.plist`, with the two absolute
paths adjusted (launchd does not expand `~` or environment variables):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>            <string>io.anuna.elephant</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Users/YOU/.local/bin/elephant</string>
    <string>daemon</string>
    <string>run</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>ELEPHANT_SYNC_INTERVAL</key> <string>30</string>
  </dict>
  <key>RunAtLoad</key>        <true/>
  <key>KeepAlive</key>        <true/>
  <key>StandardOutPath</key>  <string>/Users/YOU/Library/Logs/elephant-daemon.log</string>
  <key>StandardErrorPath</key><string>/Users/YOU/Library/Logs/elephant-daemon.log</string>
</dict>
</plist>
```

```bash
launchctl bootstrap gui/$UID ~/Library/LaunchAgents/io.anuna.elephant.plist
launchctl print gui/$UID/io.anuna.elephant | head   # verify it is running
launchctl kickstart -k gui/$UID/io.anuna.elephant   # restart (e.g. after reinstall)
launchctl bootout gui/$UID/io.anuna.elephant        # stop and unload
```

Notes:

- With `KeepAlive`, launchd restarts the daemon if it exits — including
  after `elephant daemon stop`. Use `launchctl bootout` to actually stop it.
- The daemon's flock means a launchd-managed daemon and a manual
  `elephant daemon start` cannot run at once; the second refuses with
  `daemon already running (pid …)`.
- Add a `RUST_LOG` key (e.g. `elephant=debug`) to `EnvironmentVariables`
  for sync-session tracing in the log file.

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
- [`specs/SPEC-003-elephant-tasks.md`](specs/SPEC-003-elephant-tasks.md) — the hence lifecycle SPL (task-verb surface retired in 0.3.0; the vocabulary stays legal and reserved).
- [`specs/SPEC-004-elephant-e2ee.md`](specs/SPEC-004-elephant-e2ee.md) — MLS end-to-end encryption.
- [`specs/SPEC-005-elephant-vocabulary.md`](specs/SPEC-005-elephant-vocabulary.md) — predicate vocabulary: `vocab`/`define`, documented explanations, the near-miss advisory.

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
daemon's loopback control plane, MLS
end-to-end encryption (including member removal with corpus-key rotation),
and the SPAKE2 join ceremony are implemented and tested (the join
choreography and the E2EE removal property are covered end-to-end). The
iroh-QUIC transport primitives are wired — durable/rendezvous endpoints, DHT
discovery, and a framed sync session — and the sync session converges two
replicas in tests over an in-memory duplex; the live loopback dial is
`#[ignore]`d (needs endpoint discovery to settle).

**Continuous P2P sync — implemented, opt-in.** With `ELEPHANT_SYNC_INTERVAL`
set, the daemon runs a roster-gated accept loop (REQ-107) and a periodic
roster-peer dialer, exchanging Loro deltas as `cbcl-elephant-sync` speech
acts; two replicas reconverge to equal version vectors. The dialect,
roster gate, sync session, and grow-only import are covered by unit and
duplex tests. It is **opt-in** rather than on-by-default because the live
iroh-QUIC dial cannot be exercised in CI (its loopback test is `#[ignore]`d
pending endpoint discovery); the composed pieces are proven over an in-memory
duplex.

**Not yet:** corpus encryption is symmetric-at-rest per member (no
storage-level adversary model), and the SPAKE2∘MLS∘keybook composition
carries an inherited Tier-1 crypto-review open item from
[SPEC-047](specs/SPEC-004-elephant-e2ee.md). LLM-orchestration features from
hence (`agent spawn/watch`, `plan translate/decompose`) are deliberately
deferred. See each spec's Orientation → Open section.

## References

- McCarthy, J. *Elephant 2000: A Programming Language Based on Speech Acts.*
  Stanford, 1989. <https://www-formal.stanford.edu/jmc/elephant.pdf>
- Nute, D. *Defeasible Logic.* (the SPINdle lineage spindle-rust implements)
- O'Connor, H. *CBCL: Safe Self-Extending Agent Communication.* LangSec 2026.
- RFC 9420 (MLS), RFC 9382 (SPAKE2), BEP-44 (Mainline DHT mutable items).

## License

Apache-2.0.
