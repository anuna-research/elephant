---
id: SPEC-001
title: elephant — speech-act coordination on a shared defeasible theory
version: 0.2.0
status: approved
date: 2026-07-11
last-updated: 2026-07-12
audience: agent, human reviewer
---

# SPEC-001 — elephant core

## Orientation

Intent: A CLI tool where agents (humans, LLMs, bots) coordinate by
exchanging signed speech acts into shared, append-only logical theories;
whether a task is done, a promise is kept, or a claim stands is *derived*
by defeasible reasoning, never decreed by a status column.
[[Elephant 2000]] made computable: "I meant what I said, and I said what I
meant — an elephant never forgets."

Metaphor: a notary's minute-book shared by a working group. Every utterance
is signed and entered forever; the current state of affairs is whatever the
book, read as a whole, entails today.

Structure:

```
       ┌────────────── elephant (this repo) ───────────────┐
       │  CLI (clap)          daemon (SPEC-002)            │
       │      │                   │                        │
       │      ▼                   ▼                        │
       │  ┌───────────────────────────────────┐            │
       │  │ theory store — one LoroDoc/theory │  CON-002   │
       │  │ append-only list of Entries       │            │
       │  └───────────────┬───────────────────┘            │
       │                  │ verify → parse → filter        │
       │  ┌───────────────▼───────────────────┐            │
       │  │ PURE CORE  closure(corpus,trust,t)│  CON-003   │
       │  │ envelope · tombstone · commitments│            │
       │  └───────┬──────────┬──────────┬─────┘            │
       └──────────┼──────────┼──────────┼──────────────────┘
            ┌─────▼───┐ ┌────▼─────┐ ┌──▼─────────┐
            │ cbcl-rs │ │ spindle- │ │ did-crdt   │
            │ dialect │ │ rust SPL │ │ identity   │
            │ R1–R4   │ │ closure  │ │ (core only)│
            └─────────┘ └──────────┘ └────────────┘
       arrows point inward: shell → pure core → vendor libs
```

Decisions: [[#ADR-001]] reuse `cbcl-elephant` dialect verbatim ·
[[#ADR-002]] Loro list-of-entries corpus · [[#ADR-003]] did:crdt identity,
keys ours · [[#ADR-006]] retraction as same-signer tombstone ·
[[#ADR-007]] commitment states computed at the elephant layer ·
[[#ADR-012]] provenance-from-envelope claims wrapping ·
[[#ADR-013]] single-stratum commitment-state reflection (v0.2).

Load-bearing: [[#REQ-005]] assert · [[#REQ-006]] retract ·
[[#REQ-007]] promise · [[#REQ-015]] commitment states ·
[[#REQ-021]] closure determinism · [[#REQ-022]] merge-time validation.

Open: wire `query`/`justify` performatives unused in v0.1
([[#ADR-010]], owner HOC) · revocation of membership
([[SPEC-002-elephant-p2p#ADR-104]], owner HOC) · v0.2 commitment-state
reflection ([[#REQ-025]]–[[#REQ-027]]) specified but unimplemented
(owner HOC).

Detail: the rest of this document; P2P/daemon in [[SPEC-002-elephant-p2p]];
user intent in [[users/operator/user]], [[users/agent/user]] and their
happy paths.

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD
NOT, RECOMMENDED, MAY, and OPTIONAL in this document are to be interpreted
as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they
appear in all capitals.

## 1. Overview

elephant is a Rust CLI + daemon. An **agent** is a [[DID]]-holding signer.
A **[[Logical Theory]]** (short: theory) is an independent, shared,
append-only corpus of signed [[Speech Act]] messages plus the defeasible
closure derived from it. One agent participates in many disjoint theories
([[#REQ-018]]).

The intent-anchor is McCarthy's [[Elephant 2000]]:

- Speech acts are the I/O language → the [[cbcl-elephant]] CBCL dialect
  (assert, retract, query, concede, commit, request, justify) carrying
  [[SPL]] sentences.
- "Refer directly to the past" → the corpus is the history list; every
  predicate is a function of it; nothing is ever deleted.
- "Promises should be kept" → `commit` creates a [[Commitment]] whose
  existence follows McCarthy's axiom (arisen ∧ not since revoked) and whose
  fulfilment/violation is derived, mechanically, from the corpus.
- Circumscription's role is played by [[Defeasible Logic]] closure
  (spindle-rust, ambiguity-blocking SDL, tags +D/+d/-d/-D).

The stack (all four sibling repos are consumed as path dependencies,
Simplicity Ladder rung 4 — nothing here reimplements them):

| Layer | Library | Used for |
|---|---|---|
| Wire language | `cbcl-core` + `cbcl-parser` (`../cbcl-rs`) | dialect, canonical bytes, R1–R4 |
| Reasoning | `spindle-core` + `spindle-parser` (`../spindle-rust`) | SPL, claims, trust, closure, queries |
| Identity | `did-crdt` (`../did-crdt`, default features = pure core) | did:crdt DIDs, DID documents |
| Corpus CRDT | `loro` (crates.io 1.13) | replicated append-only entry list |

## 2. Requirements

### Identity

#### REQ-001: Identity creation

The CLI SHALL create a local agent identity on `elephant id create
[--name NAME]`: generate an Ed25519 keypair, derive a `did:crdt` DID and
genesis [[DID Document]] via did-crdt `Document::new`, and persist key,
DID document, and profile under the elephant state directory WITH file
mode 0600 for the key.

Trace: [[#TEST-001]] · [[#CON-005]] · [[#ADR-003]]

#### REQ-002: Identity display

The CLI SHALL print the DID, name, key path and public key on
`elephant id whoami`, FOR both TTY and `--json` output, WITHOUT reading
any network.

Trace: [[#TEST-002]] · [[#CON-004]]

### Theories

#### REQ-003: Theory creation

The CLI SHALL create a new theory on `elephant theory create <name>`:
mint a genesis Entry (a signed `assert` of the theory's `(meta …)`
self-description including creator DID and creation time), derive the
theory id as `blake3(genesis-entry-canonical-bytes)`, and initialise the
theory's [[Loro]] document in the local store. (v0.2) The genesis meta
SHALL declare the theory's closure-semantics version — always
`(closure 2)` for newly created theories (minting closure-1 theories
is not supported: no pre-gate binary has been released to need them);
absence means closure 1 ([[#REQ-025]] · [[#ADR-013]]).

The local alias — `<name>` here, and the joiner's `--alias` or the
steward-proposed alias at adopt time
([[SPEC-002-elephant-p2p#REQ-104]]; the latter is peer-controlled wire
bytes) — SHALL match the [[LDH label]] grammar:

```abnf
alias   = let-dig / ( let-dig *61( let-dig / "-" ) let-dig )
let-dig = %x61-7A / DIGIT   ; lowercase letters and digits only
; 1–63 octets; adjacent hyphens ("--") additionally refused
```

This is [[RFC 1035]] §2.3.1's preferred name syntax as relaxed by
[[RFC 1123]] §2.1 (digit may lead), with [[RFC 6335]] §5.1's hyphen
placement rules. Lowercase-only rather than case-mapped comparison:
per [[PROTO-001]] LangSec, malformed input is rejected, never
normalised (an uppercase input is refused with the lowercase spelling
suggested); [[RFC 8265]]'s PRECIS `UsernameCaseMapped` profile is the
upgrade path if Unicode aliases are ever admitted. The 63-octet cap
keeps the alias language DISJOINT from theory ids (exactly 64 lowercase
hex), so `-t <arg>` dispatch is decided by grammar alone — RFC 1123
§2.1's "check the syntax before lookup" rule — never by filesystem
probing. Enforced at create, at adopt, and at open/dispatch.

Trace: [[#TEST-003]] · [[#CON-002]]

#### REQ-004: Theory listing

The CLI SHALL list all locally-held theories on `elephant theory list`
WITH theory id, local alias, entry count, and member count.

Trace: [[#TEST-004]] · [[#CON-004]]

### Speech acts (producers)

#### REQ-005: Assert

The CLI SHALL, on `elephant assert '<spl>' -t <theory>`: (1) fully parse
`<spl>` with spindle-parser and reject on any parse error before signing
or storing anything; (2) wrap the statement in a [[cbcl-elephant]]
`assert` performative with a fresh sentence-id; (3) wrap per [[#CON-003]]
(signed → with-limits → lang), sign with the agent key; (4) append the
Entry to the theory's corpus; and (5) print the sentence-id as receipt.

A bare literal argument (no parentheses) SHALL be sugared to
`(given <literal>)`.

Trace: [[#TEST-005]] · [[#CON-001]] · [[#CON-002]] · [[#CON-003]]

#### REQ-006: Retract

The CLI SHALL, on `elephant retract <sentence-id> [--reason R] -t <theory>`,
append a signed `retract` Entry naming the target sentence-id. At closure
time a retraction SHALL exclude its target statement from the SPL theory
IF AND ONLY IF the retraction's signer equals the target Entry's signer
(invariant E1); non-matching retractions remain in the corpus for audit
WITH no closure effect. The corpus itself never shrinks.

Trace: [[#TEST-006]] · [[#ADR-006]]

#### REQ-007: Promise

The CLI SHALL, on `elephant promise '<goal>' [--when '<trigger>']
[--by <rfc3339>] -t <theory>`, append a signed `commit` Entry whose
trigger-conditions default to `true` when `--when` is omitted, carrying
the optional deadline as commitment metadata. A `--by` timestamp in the
past SHALL be rejected before signing.

Trace: [[#TEST-007]] · [[#ADR-007]] · [[users/operator/happy-paths#HP-O4]]

#### REQ-008: Request

The CLI SHALL, on `elephant request <did-or-name> '<goal>'
[--when '<trigger>'] -t <theory>`, append a signed `request` Entry
addressed to the named agent, asking for a commitment with the given
trigger and goal.

Trace: [[#TEST-008]]

#### REQ-009: Concede

The CLI SHALL, on `elephant concede '<literal>' --re <sentence-id>
-t <theory>`, append a signed `concede` Entry accepting the referenced
statement.

Trace: [[#TEST-009]]

### Reasoning (consumers — all local, no wire queries)

#### REQ-010: Status

The CLI SHALL, on `elephant status -t <theory>`, run the closure pipeline
([[#CON-003]]) and print every conclusion with its tag (+D/+d/-d/-D);
WITH `--trust` it SHALL additionally print trust-weighted degrees and
whether each conclusion clears the applicable threshold.

Trace: [[#TEST-010]] · [[#REQ-021]]

#### REQ-011: Explain

The CLI SHALL, on `elephant explain <literal> -t <theory>`, print the
derivation (rules fired, premises, provenance source per rule) for a
provable literal, or state that it is not provable.

Trace: [[#TEST-011]]

#### REQ-012: Why-not

The CLI SHALL, on `elephant why-not <literal> -t <theory>`, print the
blocking conditions for a non-provable literal: candidate rules, held and
missing premises, and defeaters that fired.

Trace: [[#TEST-012]]

#### REQ-013: Require (abduction)

The CLI SHALL, on `elephant require <literal> -t <theory>`, print a
minimal fact set whose addition would make the literal defeasibly
provable, marking results as possibly partial when the search is bounded.

Trace: [[#TEST-013]]

#### REQ-014: What-if

The CLI SHALL, on `elephant what-if <fact>… <literal> -t <theory>`,
evaluate the hypothesis non-destructively and print the goal's resulting
status plus all newly-provable and changed conclusions, WITHOUT writing
anything to the corpus.

(v0.2) Hypothesis facts are injected into the *base* theory — commitment
states recompute under the hypothesis, then reflection re-runs — EXCEPT
a `commitment-state` hypothesis, which is the one lawful user-supplied
use of the reserved predicate: it REPLACES the reflected fact for that
commitment id in the final pass (the *recomputed* one, when base-fact
hypotheses also changed it), letting a member probe "what if this
promise were violated?" without contradiction against the reflected
state ([[#REQ-025]] · [[#ADR-013]]). Edge rules: an id with no
reflected fact (nonexistent or quarantined commit — `retracted` ones
do reify) → exit 8; two hypotheses naming the same id → exit 1; a
state atom outside the [[#REQ-015]] five → exit 3.

Trace: [[#TEST-014]] · [[#TEST-028]]

#### REQ-015: Commitment states

The CLI SHALL, on `elephant commitments -t <theory>`, list every
commitment in the corpus with a state computed per McCarthy's axiom:

| State | Definition (at evaluation time t) |
|---|---|
| `retracted` | commit Entry retracted by its own signer before t |
| `pending` | exists ∧ trigger-conditions not defeasibly provable |
| `outstanding` | exists ∧ trigger +d ∧ goal not +d ∧ (no deadline ∨ deadline ≥ t) |
| `fulfilled` | exists ∧ trigger +d ∧ goal +d |
| `violated` | exists ∧ trigger +d ∧ goal not +d ∧ deadline < t |

where *exists* ≡ the commit Entry has arisen and not since been retracted
(same-signer). A `fulfilled` state, once evidenced by a corpus whose
closure derives it, SHALL NOT regress to `outstanding` merely by later
time passing (evaluation is monotone in the corpus, not in wall-clock).

Trace: [[#TEST-015]] · [[#ADR-007]] · [[Elephant 2000]]

#### REQ-016: Journal

The CLI SHALL, on `elephant log -t <theory>`, print every corpus Entry in
HLC order — sentence-id, signer, performative, payload, timestamp,
verification state — including Entries excluded from closure (retracted,
invalid-at-merge held in quarantine), each labelled with why.

Trace: [[#TEST-016]] · [[#REQ-022]]

### Cross-cutting

#### REQ-017: Watch

The CLI SHALL, on `elephant watch <literal> -t <theory>`, stream a
notification line each time the literal's tag changes as new Entries
merge, until interrupted. (Transport of remote entries is
[[SPEC-002-elephant-p2p]]; with no daemon this watches local appends.)

(v0.2) On closure-2 theories, watch SHALL additionally re-evaluate at
the deadline of every non-retracted commitment (not only currently
`outstanding` ones — a `pending` commitment's trigger may flip before
its deadline), so a tag change caused by pure time passage (a state
flipping to `violated` and its consuming rules firing, [[#REQ-025]])
is streamed without waiting for a new Entry. This wakeup set is exact
for theories free of temporal SPL forms; boundaries of temporal
literals (`during`, Allen relations) are NOT tracked in v0.2 — a tag
driven by one may lag until the next merge or deadline wakeup.
`watch --at` is a usage error (exit 1): watch observes the live
present.

Trace: [[#TEST-017]] · [[#TEST-028]]

#### REQ-018: Disjoint multi-theory participation

The store SHALL keep each theory's corpus, closure, membership, and trust
fully isolated: no Entry, literal, rule, or trust statement of one theory
SHALL influence the closure of another. One identity participates in any
number of theories.

Trace: [[#TEST-018]] · [[users/operator/happy-paths#HP-O6]]

#### REQ-019: Machine output

Every command SHALL support `--json` emitting a stable, documented JSON
shape on stdout ([[#CON-004]]); errors under `--json` SHALL go to stderr
as JSON objects; human TTY output is explicitly not a stable contract.

Trace: [[#TEST-019]] · [[#CON-004]] · [[users/agent/happy-paths#HP-A1]]

#### REQ-020: Offline-first

Every producer command ([[#REQ-005]]–[[#REQ-009]]) SHALL succeed with no
network available, appending locally; synchronisation is asynchronous and
eventual ([[SPEC-002-elephant-p2p]]).

Trace: [[#TEST-020]]

#### REQ-021: Closure determinism

The closure pipeline SHALL be a pure function: identical (corpus,
local trust policy, evaluation time) SHALL yield identical conclusions
and commitment states, on any replica, in any merge order.

Trace: [[#TEST-021]] (property test) · [[#CON-003]]

#### REQ-022: Merge-time validation

An Entry SHALL be admitted to a corpus replica only if: (1) its signature
verifies against the signer's DID key; (2) its bytes parse as canonical
CBCL and satisfy R1–R4; (3) the inner message is a [[cbcl-elephant]]
performative; (4) its payload parses as valid SPL (for `assert`) or has
the declared argument shape (other performatives); (5) (v0.2, closure-2
theories only) its payload satisfies the reserved-namespace position
rule of [[#REQ-026]], the single-stratum rule of [[#REQ-027]], contains
no inline `claims` block ([[#ADR-012]], now enforced at merge, not only
at producer parse), and no `trusts`/`decays`/`threshold` naming the
reserved `interpreter` source.
Entries failing any check SHALL be quarantined (retained, flagged,
excluded from closure, visible in [[#REQ-016]]) — fail closed, never
repair.

Trace: [[#TEST-022]] · [[#CON-002]] · LangSec principles

#### REQ-023: Local trust policy

The CLI SHALL apply a per-theory local trust policy — `(trusts <source>
<w>)`, `(threshold <name> <w>)`, `(decays …)` statements from the
agent's local configuration — to closure WITHOUT publishing them to the
corpus; publishing trust statements INTO the corpus (as ordinary asserts)
SHALL also be possible and affects every member's closure input.
(v0.2, closure-2 theories: the one atom neither channel may set is the
reserved `interpreter` source — [[#REQ-026]].)

Trace: [[#TEST-023]] · [[#ADR-012]]

#### REQ-024: Exit codes

The CLI SHALL exit 0 on success and use distinct documented codes FOR:
usage error (1), configuration/identity error (2), SPL/CBCL parse or
validation rejection (3), signature/verification failure (4), E1
violation surfaced to the producer (5), reasoner resource exhaustion (6),
transport/daemon error (7), not-found (8).

Trace: [[#TEST-024]] · [[#CON-004]]

### Commitment-state reflection (v0.2 — specified, not yet implemented)

#### REQ-025: Commitment-state reflection

The closure pipeline SHALL, after commitment-state evaluation
([[#REQ-015]]), reify each commitment's state as a fact

```
(commitment-state <sentence-id> <state>)
```

(sentence-id of the `commit` Entry; state from the [[#REQ-015]] table)
and evaluate ONE further closure pass over the base theory plus the
reified facts — [[Stratified Negation|stratified evaluation]], never
iterated. Conclusions reported by every consumer command
([[#REQ-010]]–[[#REQ-014]], [[#REQ-017]]) come from this final pass.
Commitment states themselves SHALL be computed from the base
(first-pass) closure only: a trigger or goal literal provable only via
reflection alters no commitment's state — fulfilment requires
base-level evidence ([[#ADR-013]]). `explain` of a reflected fact
itself SHALL present it as interpreter-derived (commitment id, state,
evaluation time) — there is no rule chain to show.

Reflection SHALL apply only to theories whose genesis `(meta …)`
declares `(closure 2)` ([[#REQ-003]]); a theory without the declaration
keeps v0.1 closure semantics unchanged, and a replica encountering a
declared closure version it does not implement SHALL refuse to open or
evaluate that theory (exit 2, fail closed) — this is what keeps
[[#REQ-021]]'s "any replica" clause true under version skew, among
gate-implementing replicas ([[#ADR-013]] scopes the guarantee).

Trace: [[#TEST-028]] · [[#TEST-021]] · [[#ADR-013]] · [[#CON-003]]

#### REQ-026: Reserved reflection namespace

On closure-2 theories ([[#REQ-003]]; closure-1 theories keep v0.1
semantics, where the predicate is ordinary), `commitment-state` SHALL
be a reserved predicate admitted in exactly ONE position of
user-supplied SPL: as a positive body literal — a direct conjunct of
the antecedent — of a rule or defeater. EVERY other occurrence — a
bare or `given` fact, any head position (rule or defeater), any
negated form, any modal or temporal wrapping, any metadata/annotation
position, or nested as a term argument of another predicate — SHALL be
refused after full recognition, before signing or storing (exit 3), by
every producer command; a wire Entry violating the same rule SHALL be
quarantined at merge ([[#REQ-022]] check 5).
Consuming states is the feature; minting, negating, or attacking them
is forbidden — a whitelist, per LangSec: reject what the grammar of
admissible positions does not expressly allow. The `interpreter` source
atom is reserved likewise: corpus-published `trusts`/`decays`/
`threshold` statements naming it SHALL be refused/quarantined the same
way, and the pipeline-supplied `(trusts interpreter 1.0)` SHALL
override any local-config statement naming it ([[#ADR-013]]).

Trace: [[#TEST-029]] · [[#ADR-013]] · [[#CON-001]] · [[#REQ-022]]

#### REQ-027: Single stratum

On closure-2 theories (closure-1 keeps v0.1 semantics, as in
[[#REQ-026]]), `elephant promise` and `elephant request` SHALL reject
before signing (exit 3) a trigger or goal that references any
`commitment-state` literal, and a wire `commit` or `request` Entry
whose trigger or goal references the namespace SHALL be quarantined at
merge ([[#REQ-022]] check 5). A quarantined `commit` is not a commitment: it is excluded
from [[#REQ-015]] listing and appears only in the journal
([[#REQ-016]]) with its reason. Commitments about other commitments'
states are out of scope for v0.2 ([[#ADR-013]] records the upgrade
path).

Trace: [[#TEST-030]] · [[#ADR-013]] · [[#REQ-022]]

### Non-functional requirements

#### NFR-001: Closure latency

Closure (corpus → conclusions, [[#CON-003]]) SHALL complete in ≤ 250 ms
UNDER a 1 000-Entry theory on commodity hardware WITH 95th percentile,
measured by the benchmark in [[#TEST-025]].

#### NFR-002: Producer latency

`elephant assert` SHALL complete locally in ≤ 100 ms UNDER a 1 000-Entry
theory WITH 95th percentile (daemon absent, direct store).

#### NFR-003: Corpus scale

All commands SHALL remain functional (no crash, no quadratic blow-up
beyond [[#NFR-001]]×20) UNDER 10 000 Entries per theory. Larger corpora
are out of scope for v0.1.
// ceiling and upgrade path recorded in [[#ADR-002]]

#### NFR-004: Secret hygiene

Private keys SHALL be created 0600 and never written to argv, env,
logs, or JSON output; invite secrets are TTY-only
([[SPEC-002-elephant-p2p#REQ-104]]).

#### NFR-005: Single-binary footprint

The tool SHALL ship as one static-ish binary `elephant` (CLI and daemon
in one executable) built by `make build`.

## 3. Architecture decisions

#### ADR-001: Reuse the shipped `cbcl-elephant` dialect verbatim

**Decision.** The wire language is `cbcl-rs/dialects/elephant.cbcl`
(dialect `cbcl-elephant`, 7 performatives), loaded at startup by content;
its sha256 dialect hash is part of the protocol identity. elephant-3000
does not define a new dialect.

**Context.** The prior elephant design (v0.3, `../elephant/docs/`)
specified a 2-performative `elephant:task` dialect (`assert`,
`retract-claim`). Since then cbcl-rs shipped `cbcl-elephant`, which is
strictly richer (adds `query`, `concede`, `commit`, `request`, `justify`)
and is expressly modelled on [[Elephant 2000]] — `commit`/`request` are
exactly the promise/request speech acts the paper centres on.

**Trade-offs.** (+) rung 4 reuse; R3-safe by construction (templates
expand to core `tell`/`ask`); commitments become first-class wire acts
rather than an SPL encoding convention. (−) the dialect ships unsigned
(installs as `R4Result::Unsigned`) — acceptable because the binary pins
the file's dialect hash; (−) `retract` takes `(sentence-id reason)` and
lacks the partition argument of the old design — partition scoping moves
to the Entry envelope (theory id), which is cleaner anyway.

#### ADR-002: Corpus = one LoroDoc per theory, append-only entry list

**Decision.** Each theory is a [[Loro]] `LoroDoc` with a single list
container `"corpus"`; each element is one serialized Entry
([[#CON-002]]). Elephant only ever inserts — never edits or deletes —
so merge semantics degenerate to a G-set union with a stable total order
(HLC, tie-broken by signer DID), and closure stays a pure function of
the set (REQ-021).

**Context.** The prior design used did-crdt's G-set + iroh-gossip; the
goal directs Loro. Loro gives version-vector delta sync, subscriptions,
snapshot persistence, and a maintained engine shared with the zetl
direction ([[SPEC-047]]).

**Trade-offs.** (+) delta sync and persistence for free; ephemeral store
available later for presence. (−) Loro's rich CRDT power is unused —
deliberately: restricting to grow-only inserts is what keeps CALM
convergence and audit permanence. Loro peer id is derived from the low
64 bits of blake3(agent pubkey) to guarantee uniqueness.
// SIMPLIFY: full corpus kept in one Loro list; ceiling ~10k entries
// (NFR-003); upgrade path: shallow snapshots + per-epoch docs.

#### ADR-003: Identity = did:crdt core; key custody is elephant's

**Decision.** Use `did-crdt` with default (empty) features: `Did`,
`Document`, delta signing/verification. elephant owns key generation
(`ed25519-dalek` + OS RNG), storage (state dir, 0600), and implements
`cbcl_core::r4::Signer` over the same key. The DID document (serialized
via `Document::to_bytes`) travels to peers at join so signatures are
verifiable offline.

**Trade-offs.** (+) zero networking deps from did-crdt; W3C-compliant
DIDs; rotation/revocation machinery available later. (−) we must ship
the signer implementation and enforce the did-crdt HLC `node_id =
node_id_from_pubkey(...)` binding wherever we mint DID-document deltas.

#### ADR-004 → moved to [[SPEC-002-elephant-p2p#ADR-101]] (transport) and [[SPEC-002-elephant-p2p#ADR-102]] (join)

#### ADR-005: Single crate, zetl-style repo layout

**Decision.** One cargo package `elephant` (lib + bin), edition 2024
(spindle-core requires it), stable toolchain, Makefile with zetl's core
target names (`build`, `test`, `check`, `lint`, `fmt`, `fmt-fix`,
`install`, `clean`, `doc`, `help`). specs/, plans/, docs/, users/,
bugs/, tests/ at root; the repo root is a zetl vault.

**Trade-offs.** (+) matches the sibling repos' conventions; single
binary satisfies [[#NFR-005]]. (−) a workspace split (core/cli/daemon)
would enforce the purity boundary by crate — deferred; the boundary is
enforced by module structure and review instead.
// SIMPLIFY: module-level purity boundary, not crate-level; ceiling: if
// the pure core grows past ~5k LOC, split crates (trace: ADR-005).

#### ADR-006: Retraction = elephant-layer tombstone (E1), corpus monotone

**Decision.** As in the prior design: SPL stays monotonic (spindle never
sees a retract form), the corpus never shrinks, and the `retract`
performative acts as a closure-time filter honoured only when the
retraction signer equals the target signer.

#### ADR-007: Commitment states computed in the pure core, not as synthetic SPL rules

**Decision.** McCarthy's existence axiom and the state table of
[[#REQ-015]] are evaluated in Rust over (a) commit/retract Entries and
(b) the spindle closure's tags for trigger and goal literals. No
negation-as-failure rules are injected into the theory.

**Context.** Deriving `violated` needs "goal *not* provable after
deadline" — a statement about the closure, not in it. Encoding that as
SPL defeaters would conflate object- and meta-level and fight SDL's
semantics. McCarthy's own split blesses this: the interpreter "issues
the outputs… The circumscriptions are used in proving that the program
meets its specifications."

**Trade-offs.** (+) states are exact, testable, and cannot destabilise
the theory. (−) commitment states are not themselves literals other
rules can consume in v0.1; if plans need to react to `violated`, agents
assert observed states back into the theory explicitly. *The (−) is
superseded in v0.2 by [[#ADR-013]]: states are reflected into a single
second closure pass as consumable facts. The decision itself stands —
state evaluation stays in Rust, never as synthetic SPL rules.*

#### ADR-010: Wire `query` and `justify` performatives are parsed but not emitted in v0.1

Queries are local function calls (the edge-first principle of the prior
architecture: "no query crosses the wire"); `concede` is kept because it
is evidence, not a question. Entries carrying `query`/`justify` from
other implementations are valid corpus members and appear in the log but
add nothing to closure. Revisit when a remote-answering daemon mode is
specified.

#### ADR-011: Time

Entry order uses did-crdt-style HLC values embedded in the Entry
(wall_ms, logical, node_id-from-pubkey). Closure evaluation time is the
caller's wall clock (or `--at <rfc3339>` override) fed to spindle's
`PrepareOptions.reference_time` and to commitment deadline comparison —
so "as-of" queries over the never-forgotten past are a CLI flag, exactly
the Elephant 2000 promise.

#### ADR-012: Provenance is cryptographic, not self-declared

**Decision.** When building the SPL theory, the closure pipeline wraps
each admitted statement in a `(claims <source> :at … :id … :sig …)`
block generated FROM the verified envelope (signer DID → source alias,
Entry HLC → :at, sentence-id → :id, envelope signature → :sig). Inline
`claims` blocks typed by users inside an assert payload are rejected at
[[#REQ-005]] parse time.

**Context.** spindle treats claims as pure provenance annotation and
never verifies :sig; trust weighting gives unsourced rules weight 0.0.
Deriving the claims wrapper from the verified envelope means provenance
can't be spoofed below the signature layer, and every admitted statement
has a source (so trust math is total).

#### ADR-013: Commitment states reflected into a single second closure pass (v0.2)

**Decision.** Supersedes the "(−)" trade-off of [[#ADR-007]] — from
v0.2, commitment states are consumable by rules. After stage-8 state
evaluation, each state is reified as `(commitment-state <sentence-id>
<state>)`, wrapped in a `(claims interpreter :at <t>)` block with a
fixed, locally-supplied `(trusts interpreter 1.0)`, and stages 6–7
re-run exactly once over the parsed base theory extended with the
parsed reified facts ([[#CON-003]] stage 9). Reported conclusions come from this final pass; reported
commitment states come from the base pass. [[#ADR-007]]'s core decision
stands: no [[Negation-as-Failure|negation-as-failure]] enters the
theory — the state table is still evaluated in Rust; only its verdicts
are fed back as ordinary facts.

**Context.** Plans need to react to a violated or fulfilled promise
(escalation, re-planning). The v0.1 workaround — agents asserting
observed states — makes the most basic deontic reaction depend on an
observer daemon being up, opens a replica-disagreement window until the
observation syncs, and *decrees* into the corpus what the system's
pitch says should be *derived*. States are already a deterministic pure
function of (corpus, evaluation time), so feeding them back as inputs
is [[Stratified Negation|stratified evaluation]], not self-reference:
the "not provable" question is answered at stratum 0, frozen into
facts, and stratum 1 consumes the answer without being able to
influence it. [[#REQ-021]] convergence is preserved — final-pass
conclusions at time t remain a pure function of (corpus, trust, t).

**Why exactly one stratum.** The ingest-time bans ([[#REQ-026]],
[[#REQ-027]]) make the stratification structural rather than checked
per evaluation: no fixpoint iteration, no cycle detector, no
termination argument, no new member-triggerable closure pathology
surface (cf. [[BUG-001]]). Lifting an ingest ban later is a
backwards-compatible relaxation; shipping fixpoint semantics now is
complexity that could never be removed. The reified facts never cross
the wire — every replica re-derives them identically — so they carry
the one non-cryptographic source [[#ADR-012]] admits: deterministic
re-derivation is not testimony.

**The `interpreter` source is reserved and cannot collide.** Member
source atoms are full DIDs (the 0.1.1 widening), so no signer can *be*
`interpreter`; what a member could do is publish `(trusts interpreter
0.0)` into the corpus ([[#REQ-023]]) and mute every reflected fact on
every replica — a violator silencing the reaction to their own
violation. Hence [[#REQ-026]] reserves the atom in published trust,
and the pipeline-supplied `(trusts interpreter 1.0)` overrides local
config: the reflection trust anchor is a constant of the semantics,
not an opinion. Inline `(claims …)` spoofing is closed on both paths:
ADR-012 rejects it at producer parse time, and [[#REQ-022]] check 5
quarantines wire-crafted payloads carrying inline claims blocks —
producer-side rejection alone would leave the reserved 1.0 anchor as
the highest-value wire forgery.

**Version skew is gated at the theory, fail closed — among replicas
that implement the gate.** A v0.1 closure admits SPL this ADR
quarantines; two binary versions over one corpus would derive different
conclusions, violating [[#REQ-021]]'s "any replica" clause. So
reflection and the [[#REQ-022]] check 5 activate only for theories
whose signed genesis meta declares `(closure 2)` ([[#REQ-003]],
[[#REQ-025]]) — the declaration is inside the bytes the theory id is
derived from, so it cannot be retrofitted or forked silently — and a
replica that does not implement a theory's declared closure version
refuses to open it (exit 2). The gate is one-directional: a binary
predating it contains no version check and would evaluate a closure-2
theory with v0.1 semantics. That skew is accepted for v0.2 because no
0.1.x binary has been released — the gate is binding from the first
release onward; a closure-version floor in the SPEC-002 join handshake
is the recorded upgrade path if pre-gate binaries ever circulate
(owner HOC). Undeclared theories keep v0.1 semantics forever;
upgrading one means creating a closure-2 successor theory (membership
carries over by re-join; corpus migration is out of scope for v0.2).

**Fulfilment is base-evidence-only.** A rule consuming one commitment's
state can make *another* commitment's trigger or goal provable in the
final pass ([[#REQ-027]] bans only direct reference). If states were
read from the final pass, states would depend on states — the cycle the
single stratum exists to exclude. So [[#REQ-025]] fixes: state
evaluation reads base-pass tags only. A promise is discharged by signed
evidence in the corpus, never by a conclusion that exists only because
some other promise's verdict was reflected. The observable consequence
is deliberate and documented: `status` can show a reflection-derived
literal `+d` while a commitment with that literal as goal stays
`outstanding` — `explain` names the reflected premise, making the
stratum visible.

**Violated is curable.** The [[#REQ-015]] table is unchanged: a goal
proven after the deadline reads `fulfilled`, and final-pass conclusions
that fired off `violated` retract — the defeasible reading (late
delivery cures the escalation). "Was violated at t" stays answerable
via `--at <t>` ([[#ADR-011]]) and the journal ([[#REQ-016]]); a
monotone `commitment-violated-at` reification is deferred until a REQ
demands irreversible consequences (rung 1, YAGNI).

**Trade-offs.** (+) reactions to promise outcomes are derived, offline,
deterministic — no observer agent, no lag, no decree. (+) zero new
logic semantics: spindle is unchanged; elephant feeds it one more pass.
(−) closure cost approaches ×2 if stage 9 naively re-assembles and
re-parses; the [[#NFR-001]] 250 ms budget now covers both passes
(v0.1 bench: ~131 ms at 1 k entries), so the implementation MUST reuse
the parsed base theory and append only the reified facts —
[[#TEST-025]] gates this. (−) commitments about commitments are
rejected, not supported ([[#REQ-027]]); upgrade path: stratification
check over the commitment dependency graph, then bounded fixpoint.
(−) the base/final split is a second closure surface to explain;
mitigated by `explain` showing reflected premises.

## 4. Contracts

#### CON-001: SPL payload grammar

Input: the `<spl>` argument of [[#REQ-005]] and rule/trigger/goal
arguments elsewhere. Grammar: SPL per
`spindle-rust/docs/src/reference/spl.md` (regular-to-context-free,
S-expression). Recogniser: `spindle_parser::parse_spl` — the single
parser for this language in the binary (one parser per language).
Restrictions on top of SPL, enforced after parse: no `claims` blocks
([[#ADR-012]]); no `trusts`/`decays`/`threshold` unless the command is
explicitly publishing trust ([[#REQ-023]]); `commitment-state` only as
a positive top-level rule/defeater body literal, never in promise or
request triggers/goals, and no published trust naming `interpreter`
([[#REQ-026]], [[#REQ-027]]; v0.2 — sole exception: a what-if
hypothesis per [[#REQ-014]]).
Pre: UTF-8 argument ≤ 64 KiB. Post: a validated `Theory` fragment or a
parse error naming the offending form; nothing signed or stored on error.
Error model: exit 3, JSON `{error: "parse", detail…}`.
Implements: [[#REQ-005]] [[#REQ-007]] [[#REQ-008]] [[#REQ-014]]
[[#REQ-026]] [[#REQ-027]].
Verified by: [[#TEST-005]] [[#TEST-026]] (fuzz) [[#TEST-029]]
[[#TEST-030]].

#### CON-002: Entry — the corpus element

```rust
struct Entry {
  v: u16,                    // format version = 1
  theory: TheoryId,          // blake3 hex of genesis entry bytes
  hlc: { wall_ms: u64, logical: u32, node_id: u64 },  // node_id = low64(blake3(pubkey))
  signer: String,            // did:crdt:…
  key_id: String,            // DID URL of the verification method
  cbcl: String,              // canonical CBCL text, per CON-003
  sig: [u8; 64],             // Ed25519 over signing_input
}
// signing_input = "elephant-entry-v1" ‖ theory ‖ hlc(canonical) ‖ cbcl-bytes
// sentence-id (receipt) = "s-" + hex(blake3(signing_input))[..16]
```

Serialisation into the Loro list: canonical JSON (sorted keys, no
whitespace — did-crdt's `canonical_json` discipline), one list element
per Entry. Grammar: JSON (RFC 8259) recognised by serde into the typed
struct — full recognition before any semantic action; unknown fields
rejected (`deny_unknown_fields`). Total order: (hlc, signer) —
deterministic across replicas.
Pre: element ≤ 64 KiB. Post: a verified Entry or quarantine
([[#REQ-022]]).
Implements: [[#REQ-003]] [[#REQ-005]] [[#REQ-022]].
Verified by: [[#TEST-022]] [[#TEST-027]] (roundtrip property)
[[#TEST-026]] (fuzz).

#### CON-003: Closure pipeline (pure core)

```
closure(entries: &[Entry], trust: &TrustPolicy, now: TimePoint)
  -> { conclusions: Vec<WeightedConclusion>,
       commitments: Vec<CommitmentState>,
       quarantined: Vec<(Entry, Reason)> }
```

Stages, in order, all deterministic: 1 verify signature + DID binding →
2 parse CBCL (`cbcl_parser::run_pipeline`-equivalent, dialect check
against pinned `cbcl-elephant` hash; on closure-2 theories also the
payload-restriction scan of [[#REQ-022]] check 5, so `closure()`
re-derives the full quarantine verdict from `&[Entry]` alone)
→ 3 E1 tombstone filter (`retract`)
→ 4 build SPL text: for each admitted `assert`, a claims block per
[[#ADR-012]]; plus local trust statements → 5 `parse_spl` → 6 spindle
`reason` with `reference_time = now` → 7 `compute_weighted_conclusions`
→ 8 commitment-state evaluation ([[#REQ-015]]) → 9 (v0.2, closure-2
theories) reflection: reify stage-8 states per [[#REQ-025]] with
interpreter claims + fixed trust, re-run 6–7 once over the *parsed*
base theory extended with the parsed reified facts (no re-assembly, no
re-parse of the base — [[#NFR-001]] covers both passes); final
conclusions come from this pass, commitment states remain stage 8's
([[#ADR-013]]).
The function does no I/O; `now` is a parameter (never `SystemTime::now`
inside), which is what makes [[#REQ-021]] testable.
Implements: [[#REQ-010]]–[[#REQ-016]] [[#REQ-021]] [[#REQ-025]].
Verified by: [[#TEST-021]] [[#TEST-010]]–[[#TEST-016]].

#### CON-004: CLI JSON contract

Stable shapes (v1): every success object carries `"v": 1` and the
`theory` id where applicable. Representative shapes:

```json
// assert / promise / request / concede / retract
{ "v":1, "receipt":"s-3b8a9c1d0e2f4a6b", "theory":"…", "signer":"did:crdt:…",
  "performative":"assert", "spl_form":"given" }
// status (array elements); v0.2: a reflection-derived conclusion
// carries "interpreter" among its sources
{ "literal":"release-ready", "tag":"+d", "degree":0.9,
  "above_threshold":true, "sources":["did:crdt:…"] }
// commitments (array elements)
{ "id":"s-…", "by":"did:crdt:…", "trigger":"true", "goal":"legal-signed",
  "deadline":"2026-07-18T17:00:00Z", "state":"outstanding" }
// watch (NDJSON stream)
{ "theory":"…", "literal":"release-ready", "old":"-d", "new":"+d",
  "at":"2026-07-11T09:00:00Z" }
// error (stderr)
{ "error":"parse", "exit":3, "detail":"…" }
```

Implements: [[#REQ-019]] [[#REQ-024]]. Verified by: [[#TEST-019]].

#### CON-005: State directory layout

```
${ELEPHANT_HOME:-$XDG_DATA_HOME/elephant}/
  identity/{key.ed25519  (0600), did.json, profile.toml}
  theories/<theory-id>/{doc.loro, meta.toml}     # meta: alias, joined_at
  trust/<theory-id>.spl                          # local trust statements
  daemon/{daemon.json, lock}                     # SPEC-002 (0600, flock)
```

macOS uses the XDG-equivalent under `~/Library/Application Support`
unless `ELEPHANT_HOME` overrides; all paths printed by `elephant id
whoami --json`. Implements: [[#REQ-001]] [[#NFR-004]].

## 5. Purity Boundary Map

Pure core (no I/O, no clocks, no globals): `core::envelope`
(build/verify Entry), `core::dialect` (pinned dialect load + message
shape), `core::tombstone` (E1 filter), `core::theory_text` (claims
wrapping, SPL assembly), `core::closure` ([[#CON-003]]),
`core::commitment` (state table).
Effectful shell: `store` (Loro persistence), `cli` (clap, output),
`daemon::*` (SPEC-002), `id` (keygen, files), `clock` (the only reader
of wall time).
Boundary types: `Entry`, `TrustPolicy`, `TimePoint`,
`ClosureResult`. Dependency rule: shell → core; core imports only
cbcl-*, spindle-*, did-crdt(core), blake3, ed25519-dalek, serde.
Enforcement: module visibility + clippy `disallowed-methods` denying
`std::fs`, `SystemTime::now`, `tokio` inside `core::*`.

## 6. Test specification

Strategy per the protocol's selection table: complex pure-core logic →
property-based + example tests; parsers at trust boundaries (Entry JSON,
CBCL bytes, SPL text, invite codes) → fuzzing REQUIRED; commitment state
machine → property tests over generated histories; AI-synthesised spec →
requirement-targeted decomposition (positive / negative-input /
negative-output per REQ) and adversarial pass before `implemented`.

| TEST | Validates | Positive | Negative-input | Negative-output |
|---|---|---|---|---|
| TEST-001 | REQ-001 | create → key 0600 + DID resolvable | second create w/o force refused | DID mismatch with pubkey rejected |
| TEST-002 | REQ-002 | whoami fields | missing identity → exit 2 | — |
| TEST-003 | REQ-003 | create → genesis entry verifies; id = blake3; LDH aliases accepted (1..63, digit-led ok) | duplicate alias refused; malformed alias (uppercase, hyphen at an edge, `--`, dot/space/path fragment, >63, 64-hex shape) → exit 1, nothing created | tampered genesis → id mismatch detected |
| TEST-004 | REQ-004 | list shows created+joined | — | counts wrong → fail |
| TEST-005 | REQ-005 | assert `(given x)` → receipt, corpus+1 | malformed SPL → exit 3, corpus+0 | stored entry fails verify → fail |
| TEST-006 | REQ-006 | own retract removes stmt from closure | retract of others' entry → inert (audit only) | corpus shrank → fail |
| TEST-007 | REQ-007 | promise → commitment listed | `--by` in past → exit 3 | deadline lost/garbled → fail |
| TEST-008 | REQ-008 | request entry addressed correctly | unknown addressee format → exit 3 | — |
| TEST-009 | REQ-009 | concede references target | dangling --re id → exit 8 | — |
| TEST-010 | REQ-010 | penguin example tags match spindle | — | +d where spindle says -d → fail |
| TEST-011 | REQ-011 | explain shows rule chain+source | unknown literal → clean message | — |
| TEST-012 | REQ-012 | why-not lists missing premises | — | omits firing defeater → fail |
| TEST-013 | REQ-013 | require returns minimal set | — | non-minimal/incorrect set → fail |
| TEST-014 | REQ-014 | what-if flips goal, corpus unchanged; (v0.2) state hypothesis replaces the reflected fact | (v0.2) unknown commitment id → exit 8; duplicate id hypotheses → exit 1; non-table state atom → exit 3 | corpus mutated → fail |
| TEST-015 | REQ-015 | full state table walk (5 states) | — | fulfilled regresses by clock → fail |
| TEST-016 | REQ-016 | log shows all incl. quarantined+why | — | hides retracted → fail |
| TEST-017 | REQ-017 | watch fires on local append tag flip; (v0.2) fires at a pending commitment's deadline with no new entry | (v0.2) `watch --at` → exit 1 | fires w/o tag change → fail |
| TEST-018 | REQ-018 | two theories, same literal names, independent closures | — | cross-leak → fail |
| TEST-019 | REQ-019 | every command --json parses & matches CON-004 | — | schema drift → fail |
| TEST-020 | REQ-020 | asserts succeed with store-only mode | — | network touched → fail (no net in unit env) |
| TEST-021 | REQ-021 | property: shuffle entry order ⇒ identical closure | — | divergence → fail |
| TEST-022 | REQ-022 | valid entry admitted | bad sig / bad CBCL / core performative inside dialect / bad SPL → quarantined | quarantined entry influences closure → fail |
| TEST-023 | REQ-023 | local trusts change weights not corpus | — | local trust leaked to corpus → fail |
| TEST-024 | REQ-024 | exit code matrix | — | — |
| TEST-025 | NFR-001/2/3 | criterion bench 1k/10k entries | — | p95 over budget → fail |
| TEST-026 | CON-001/002 | cargo-fuzz targets: entry JSON, SPL arg | crashes/hangs → fail | — |
| TEST-027 | CON-002 | property: serialize∘parse = id on Entries | — | — |
| TEST-028 | REQ-025 | rule over `(commitment-state s violated)` fires once deadline passes (`--at`); retracts when late delivery cures | theory declaring unimplemented closure version → exit 2, not evaluated | reflected fact disagrees with `commitments` listing at same t → fail; goal provable only via reflection changes a state → fail |
| TEST-029 | REQ-026, REQ-022(5) | rule/defeater *body* consuming a state admitted | fact, rule/defeater head, negated, modal/temporal-wrapped, annotation-position, or term-nested `commitment-state` → exit 3, corpus+0; published trust naming `interpreter` → exit 3; wire-crafted equivalents incl. inline `(claims interpreter …)` payload → quarantined (closure-2 theory); same payloads admitted unchanged on a closure-1 theory | spoofed state or muted interpreter influences closure → fail |
| TEST-030 | REQ-027 | ordinary promise and request accepted unchanged | promise or request trigger/goal naming `commitment-state` → exit 3; wire `commit`/`request` ditto → quarantined, commit absent from `commitments`, journalled with reason | — |

## 7. Observability

- OBS-001: structured `tracing` logs on the daemon and `-v` CLI runs;
  every merge decision (admit/quarantine + reason) is logged with
  sentence-id — the audit trail REQ-016 renders.
- OBS-002: `elephant daemon status --json` counters: entries merged,
  quarantined, closures run, closure p95, per SPEC-002.

## 8. Security considerations

Trust boundaries: (1) corpus entries from peers — full recognition per
[[#REQ-022]], fuzzed ([[#TEST-026]]); (2) SPL text from argv — parsed
before signing ([[#CON-001]]); (3) invite codes and network frames —
[[SPEC-002-elephant-p2p]]. Spam/flood by a member (inert retracts,
garbage asserts) is bounded by per-theory membership and surfaced by
trust weighting, not silently dropped. Cryptography used: Ed25519
(sign), blake3 (ids), SPAKE2+HKDF (join; SPEC-002) — all from existing
audited crates; no novel constructions (no-go area respected).

## Changelog

<details>
<summary>Revision history</summary>

- 0.2.0 (draft) — commitment-state reflection: one further closure pass
  consumes reified `(commitment-state <id> <state>)` facts so rules can
  react to promise outcomes ([[#REQ-025]]); reserved-namespace
  integrity — consuming states is allowed, minting them is rejected at
  parse and quarantined at merge ([[#REQ-026]]); single-stratum
  restriction on promise triggers/goals ([[#REQ-027]]); [[#ADR-013]]
  supersedes the ADR-007 consumability trade-off (fulfilment stays
  base-evidence-only; violated stays curable). Hardened by adversarial
  review round 1: reserved namespace defined positionally over SPL's
  full form inventory (negation/defeaters/modals/nesting closed);
  `interpreter` source atom reserved against published-trust muting;
  version skew gated by a genesis-declared `(closure 2)`, fail closed
  ([[#REQ-003]]); merge-time check 5 added to [[#REQ-022]]; what-if
  stratum semantics fixed ([[#REQ-014]]); watch re-evaluates at
  outstanding deadlines ([[#REQ-017]]); request symmetry ([[#REQ-027]]).
  Round 2: skew guarantee scoped honestly (gate binds only
  gate-implementing binaries; join-handshake floor is the upgrade
  path); wire-crafted inline `claims` blocks quarantined by check 5
  (closing the `interpreter`-anchor forgery ADR-012's producer-side
  rejection missed); watch wakeup widened to all non-retracted
  deadlines and scoped against temporal SPL forms; what-if edge rules
  (unknown id, duplicate ids, non-table atom); producer rejections
  closure-2-conditional. Specified only — the 0.1.2 surface remains
  implemented and unchanged; status returns to `implemented` when
  TEST-028–030 are green.

- 0.1.2 — theory aliases get a declared grammar ([[#REQ-003]]): LDH
  labels, 1–63 octets, lowercase-only, RFC 1035/1123/6335-grounded;
  disjoint from theory ids by length so `-t` dispatch is grammar-decided
  (no filesystem probing on raw input); enforced at create, adopt
  (steward-proposed alias is wire bytes), and open.

- 0.1.1 — adversarial review round: fixed genesis-sentinel over-binding (spoofed-genesis entries now quarantined), widened source_atom to the full DID (no 64-bit trust-collision), and made closure fail closed per-entry when the assembled theory is unparseable (a toxic statement is quarantined, not fatal). Regression tests added.

- 0.1.0 — implemented (identity, corpus, closure, commitments, queries). BUG-001 (duplicate-rule-label DoS) found by the convergence property test and fixed with deterministic label shadowing.

- 0.1.0 — initial specification (Phase 1–2), derived from the goal
  directive, [[Elephant 2000]], the prior `../elephant` design corpus
  (v0.2/0.3), and [[SPEC-047]]; approved for implementation as the
  stakeholder directive pre-authorised the stack.
</details>
