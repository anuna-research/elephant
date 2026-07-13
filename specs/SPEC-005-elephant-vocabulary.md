---
id: SPEC-005
title: elephant — predicate vocabulary: introspection, documentation, coining
version: 0.2.0
status: draft
date: 2026-07-13
last-updated: 2026-07-13
audience: agent, human reviewer
---

# SPEC-005 — elephant vocabulary

## Orientation

Intent: Agents coordinating through a shared theory must converge on the
*same* predicate names — a CI bot asserting `build-ok-m1` past a rule that
listens for `ci-green-m1` is a silent coordination failure. This spec makes
the theory's working vocabulary discoverable, documentable, and
drift-visible, so vocabulary agreement is *derived from the corpus* like
everything else in elephant — never decreed by an out-of-band schema.

Metaphor: a working group's glossary kept inside the minute-book itself.
Nobody is forbidden from coining a word, but every word in use is listed,
every listed word carries its definition and its author, and the clerk
taps your shoulder when you say a word nobody is listening for.

Structure:

```
   corpus entries ──────────────────────┐
   (asserts: rules · facts · (meta …))  │
                                        ▼
    ┌────── closure pipeline (SPEC-001 CON-003, unchanged) ──────┐
    │   admitted rules · facts · label metadata · conclusions    │
    └──────────┬─────────────────────────────────┬───────────────┘
               ▼                                 ▼
    ┌─ core::vocab  CON-402 ─────┐   ┌─ explanation queries ─────┐
    │ family resolution          │   │ why-not · require         │
    │ role classification        │   │ + docs join     REQ-405   │
    │ docs join · advisory calc  │   └───────────────────────────┘
    └──────────┬─────────────────┘
               ▼
    vocab (read) · define (produce) · assert advisory   REQ-401/403/406
    (advisory served by the live daemon's cached view — NFR-402)
```

Decisions: [[#ADR-401]] the theory is its own ontology — rules are the
signature, meta is the documentation; no pinned schema object ·
[[#ADR-402]] predicate families resolved structurally by suffix over the
frozen flat vocabulary; squatting is surfaced, never blocked ·
[[#ADR-403]] documentation conflicts converge by per-key
last-write-in-canonical-order, surfaced not prevented · [[#ADR-404]]
advisory, never enforcement — a near-miss warns, nothing blocks or
quarantines · [[#ADR-405]] two new flat verbs, `vocab` and `define`.

Load-bearing: [[#REQ-401]] vocabulary view · [[#REQ-402]] documentation
carrier · [[#REQ-404]] built-in registry · [[#REQ-406]] near-miss
advisory · [[#REQ-407]] redefinition provenance.

Open: opt-in genesis-declared vocabulary *enforcement* deferred with a
named trigger ([[#ADR-401]], owner HOC) · parameterised literals
(`(ci-green m1)`) as the post-freeze upgrade ([[#ADR-402]], owner HOC) ·
a `complete`-time advisory when a `verified-<task>` evidence rule exists
but is unproven (touches [[SPEC-003-elephant-tasks]] surface, owner HOC).

Detail: the rest of this document; the closure pipeline in
[[SPEC-001-elephant-core]]; the frozen task vocabulary in
[[SPEC-003-elephant-tasks]]; user intent in [[users/agent/user]],
[[users/operator/user]] and their happy paths.

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD
NOT, RECOMMENDED, MAY, and OPTIONAL in this document are to be interpreted
as described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they
appear in all capitals.

## 1. Overview

### 1.1 The agreement problem

A predicate in an elephant theory only has consequences if something
listens for it — a rule body (positive or negated) or a commitment's
trigger/goal; an asserted literal nothing consumes is inert. Agreement
on predicate names therefore decomposes into two questions: **who coins a
name** (whoever authors the rule or commitment that listens for it) and
**how everyone else discovers it** (they ask the theory). elephant
already has three agreement channels of decreasing strength:

1. **By construction** — the frozen hence lifecycle vocabulary
   ([[SPEC-003-elephant-tasks]] §1) is emitted by the CLI verbs, never
   typed by agents.
2. **Pull-based** — `why-not` returns the exact missing body literals per
   blocking rule ([[SPEC-001-elephant-core#REQ-012]]); `require` abduces
   complete fact sets that would prove a goal
   ([[SPEC-001-elephant-core#REQ-013]]). The well-behaved agent never
   invents an evidence predicate: it asks the theory what is missing and
   asserts exactly that.
3. **By negotiation** — a `commit`/`request` pair pins the goal literal
   both sides will treat as fulfilment
   ([[SPEC-001-elephant-core#REQ-007]], [[SPEC-001-elephant-core#REQ-008]]).

What is missing is the **push path**: an agent asserting spontaneously
(before any rule listens, or guessing a name — [[users/agent/happy-paths#HP-A1]]
is exactly this shape) gets no feedback that its literal fell on deaf
ears, and there is no machine-readable answer to "what predicates does
this theory speak, and what do they mean?". This spec closes that gap
with four capabilities, all *derived* from the corpus:

- a **vocabulary view** (`elephant vocab`) projecting the admitted theory
  into [[Predicate Family|predicate families]] × roles × documentation
  ([[#REQ-401]]);
- a **documentation carrier** — the existing [[SPL]] `meta` directive,
  attached to family atoms under a declared grammar ([[#REQ-402]]), with
  a validating producer verb `define` ([[#REQ-403]]);
- **enriched explanations** — `why-not`/`require` output joins each
  missing literal with its family's documentation ([[#REQ-405]]);
- a **near-miss advisory** on `assert` when a fact has no listener and
  no documentation ([[#REQ-406]]).

### 1.2 What this spec deliberately does not do

No pinned schema object, no vocabulary enforcement, no quarantine of
unknown predicates ([[#ADR-401]], [[#ADR-404]]). The rule set *is* the
per-theory ontology — signed, replicated, amendable through the same
speech-act channel as everything else, and defeasibly contested like
everything else. Openness is load-bearing: the discovery vocabulary
(`discovered- finding- insight- …`) exists precisely so agents can record
the unforeseen. A closed vocabulary would reintroduce the schema-owner
role ("the new Jira admin") this project exists to remove.

Nothing in this spec influences admission, closure, conclusions, or
commitment states. The vocabulary layer is a *projection* with two side
channels: display and advice.

### 1.3 The coining convention (RECOMMENDED authoring pattern)

Whoever authors a rule coins the predicates in its body; the convention
is to document them in the same batch:

```lisp
(normally r-verified-m1 (and ci-green-m1 review-approved-m1) verified-m1)
(meta ci-green (description "CI pipeline green for the suffixed task")
               (kind evidence) (asserter "role:ci"))
(meta review-approved (description "a human reviewer approved the change")
                      (kind evidence) (asserter "role:reviewer"))
```

Evidence rules SHOULD conclude `verified-<task>` — already part of the
frozen discovery vocabulary — never `completed-<task>`, whose
given-fact semantics are frozen by
[[SPEC-003-elephant-tasks#ADR-201]] (rationale in [[#ADR-401]]). After
this batch, every other participant reaches the names mechanically:
`require verified-m1` returns the two evidence literals *with their
descriptions* ([[#REQ-405]]); nobody guesses.

## 2. Requirements

### Introspection (consumers — all local, no wire queries)

#### REQ-401: Vocabulary view

The CLI SHALL, on `elephant vocab -t <theory>`, run the closure pipeline
([[SPEC-001-elephant-core#CON-003]]) and print the theory's vocabulary as
one row per [[Predicate Family]] ([[#CON-402]]) that occurs in the
admitted theory (rule heads, rule bodies, facts, commitment/request
triggers and goals) or carries documentation metadata per [[#REQ-402]]
(conforming or not), each row showing:

- the family name;
- its **roles** — any of `fact` (heads a fact rule), `head` (heads a
  non-fact rule or defeater), `body` (occurs in a rule or defeater body,
  positively or negated), `goal` (occurs in a non-retracted, admitted
  commitment or request trigger or goal);
- its **class** — `hole` (has role `body` or `goal`, heads no non-fact
  rule, and ≥ 1 of its body/goal-occurring ground instances is not
  defeasibly provable), `orphan` (role `fact` only, is not built-in
  ([[#REQ-404]]) and carries no conforming documentation of kind
  `discovery`), else `active`;
- its **documentation** per [[#REQ-402]]: the per-key winning values,
  `malformed` markers naming each non-conforming key, provenance per
  [[#REQ-407]]; plus a `detached` marker on any documented family atom
  that does not resolve to itself under [[#CON-402]] against the current
  declared-task set (its documentation joins no literal — surfaced,
  never repaired).

Like every consumer command, the view is evaluated under the local trust
policy and evaluation time ([[SPEC-001-elephant-core#REQ-023]]): rows
derive from the corpus alone; `hole`/`active` classification may differ
across replicas exactly where their trust policies differ. WITH `--json`
the output SHALL follow [[#CON-403]], including its ordering rule.

Trace: [[#TEST-401]] · [[#CON-402]] · [[#CON-403]]

#### REQ-402: Documentation carrier

The vocabulary layer SHALL recognise as a family's documentation the
label metadata (SPL `meta` directive) attached to the family atom,
evaluated **per key over the merged record** of the admitted theory as
assembled by the closure pipeline: for each key in the [[#CON-401]] set
(`description`, `kind`, `asserter`), the winning value ([[#ADR-403]]) is
checked against [[#CON-401]] independently; a non-conforming key is
surfaced as `malformed` and treated as absent, never repaired and never
poisoning the other keys. A family is **documented** when its
`description` key is present and conforming. Documentation rides
ordinary signed `assert` Entries — it syncs, tombstones (E1), and
quarantines like any other statement; no new corpus object, performative,
or genesis field.

Precedence: a label that names an admitted rule or defeater is rule
metadata, not family documentation — the vocabulary layer SHALL ignore
it (rule labels win; see §8 for the shadowing consequence). Meta keys
outside the [[#CON-401]] set, and metadata on non-family labels, SHALL
be ignored by the vocabulary layer and left untouched for their
existing consumers ([[SPEC-003-elephant-tasks#REQ-208]] `describe`,
task declarations).

Trace: [[#TEST-402]] · [[#CON-401]] · [[#ADR-401]] · [[#ADR-403]]

#### REQ-403: Define (producer)

The CLI SHALL, on `elephant define <family> --desc <text> [--kind
evidence|state|discovery] [--asserter <who>] -t <theory>`, fully
recognise the arguments against [[#CON-401]] — family label grammar,
description and asserter caps, kind enumeration — and additionally
refuse (exit 3) a family that (a) collides with the built-in registry
([[#REQ-404]]) or (b) does not resolve to itself under [[#CON-402]]
against the current declared-task set (documentation that could never
join a literal is refused at authoring time; the same condition arriving
from the wire is surfaced as `detached` per [[#REQ-401]], never
quarantined). Rejection SHALL occur before signing or storing anything;
on success `define` SHALL append one signed `assert` Entry whose payload
is the corresponding `(meta <family> …)` form and print the sentence-id
as receipt. Raw `elephant assert '(meta …)'` remains legal and
unvalidated against [[#CON-401]] (it is ordinary SPL per
[[SPEC-001-elephant-core#CON-001]]); `define` is the recognising
producer — the one place vocabulary documentation is checked at
authoring time.

Trace: [[#TEST-403]] · [[#CON-401]] · [[SPEC-001-elephant-core#REQ-005]]

#### REQ-404: Built-in vocabulary registry

The vocabulary layer SHALL ship the following closed built-in registry —
the frozen vocabulary of [[SPEC-003-elephant-tasks]] §1 (as emitted by
the task verbs and hence plan import) plus [[SPEC-001-elephant-core]]'s
reserved predicate. This table is normative and exhaustive; membership
is not extensible from the wire.

| Family | Ground pattern | Kind |
|---|---|---|
| `task` | `task-<task>` | control |
| `no-deps` | `no-deps-<task>` | control |
| `ready` | `ready-<task>` | control |
| `completed` | `completed-<task>` | control |
| `claimed` | `claimed-<task>` | control |
| `blocked` | `blocked-<task>` | control |
| `upstream-blocked` | `upstream-blocked-<task>` | control |
| `decomposed` | `decomposed-<task>` | control |
| `permanently-failed` | `permanently-failed-<task>` | control |
| `assign-to` | `assign-to-<task>-<agent>` | control |
| `agent-available` | `agent-<name>-available` | control |
| `claim` · `unclaim` · `block` · `unblock` | `<action>-v<N>-<task>` | control |
| `state-claimed` · `state-unclaimed` · `state-blocked` · `state-unblocked` | `<state>-v<N>-<task>` | control |
| `stale` · `timeout` | `<family>-v<N>-<task>` | control |
| `commitment-state` | `(commitment-state <id> <state>)` ([[SPEC-001-elephant-core#REQ-026]]) | control |
| `discovered` `decided` `blocked-by` `requires` `verified` `failed` `finding` `approach` `insight` `partial` | `<family>-<suffix>` | discovery |

(`failed` appears in both SPEC-003 lists; registered once, kind
`control`, because propagation rules consume it.) Built-in entries carry
fixed built-in descriptions, appear in [[#REQ-401]] output marked
`built-in`, SHALL never be classed `orphan` or trigger the [[#REQ-406]]
advisory, and SHALL NOT be definable: `define` of a colliding family
SHALL be refused (exit 3), and corpus `meta` on a built-in family name
SHALL be ignored by the vocabulary layer (the freeze is not overridable
from the wire).

Trace: [[#TEST-404]] · [[SPEC-003-elephant-tasks#ADR-201]] · [[#CON-402]]

#### REQ-405: Documented explanations

`elephant why-not <literal>` and `elephant require <literal>` SHALL join
each missing/abduced literal against the documentation of its family —
corpus documentation ([[#REQ-402]]) and built-in registry entries
([[#REQ-404]]) alike: in text output, a documented family's description
follows the literal; WITH `--json`, the existing shapes gain a `docs`
object mapping family → `{description, kind, asserter?, built_in}` for
every documented family appearing in `missing`/`solutions`
(undocumented families omitted; `documenter`/`redefined` deliberately
omitted here — provenance lives in the vocab view; shape per
[[#CON-403]]). Output for undocumented families is unchanged.

Trace: [[#TEST-405]] · [[SPEC-001-elephant-core#REQ-012]] ·
[[SPEC-001-elephant-core#REQ-013]]

### Advisory (producer-side feedback)

#### REQ-406: Near-miss advisory on assert

`elephant assert` SHALL, when (i) the payload's sole form is a fact
(`(given <lit>)`, including the bare-literal sugar of
[[SPEC-001-elephant-core#REQ-005]]), (ii) `--no-advice` is not given,
and (iii) the command is served by a live daemon ([[#NFR-402]]; direct
store mode emits no advisory), evaluate the fact against the
**reference view** — the daemon's current cached closure over entries
preceding this append — and emit AFTER the append succeeds:

- an **inert-family advisory** when the fact's family ([[#CON-402]])
  (a) has neither role `body` nor role `goal` in the reference view
  (negated occurrences count — a negatively-consumed fact is not
  inert), (b) is not built-in ([[#REQ-404]]), and (c) is not documented
  with kind `discovery`; or
- a **sibling advisory** when the family does have role `body` or
  `goal` but the asserted ground literal itself occurs in no rule body
  or commitment trigger/goal while ≥ 1 sibling instance of the same
  family is an unproven body/goal occurrence (the `ci-green-m2` vs
  `ci-green-m1` wrong-suffix case).

Either advisory SHALL list up to 5 candidate ground literals — unproven
body/goal occurrences from the reference view, those sharing the fact's
resolved task suffix first — each WITH the label of the rule (or the
sentence-id of the commitment) that listens for it, so candidates are
attributable (§8). Text mode prints to stderr; `--json` carries an
`advisory` object in the receipt ([[#CON-403]]). The advisory SHALL NOT
change the exit code, SHALL NOT prevent or delay the append beyond the
reference-view lookup, and any failure of its computation SHALL degrade
to no advisory, never to a failed assert. Because the reference view is
a cache, an advisory can be stale against entries not yet reflected in
it — it is advice, not a verdict ([[#ADR-404]]). Rule/multi-form
payloads, and all other producers, emit no advisory.

Trace: [[#TEST-406]] · [[#ADR-404]] · [[#NFR-402]] ·
[[users/agent/happy-paths#HP-A1]]

#### REQ-407: Redefinition provenance

Documentation provenance is anchored on the `description` key: a
family's **documenter** is the signer of the admitted, non-tombstoned
Entry that most recently (canonical (hlc, signer) order,
[[SPEC-001-elephant-core#CON-002]]) wrote a conforming `description`
for the family; `redefined` is true when ≥ 2 distinct signers have
admitted, non-tombstoned Entries writing a conforming `description` for
it (quarantined and E1-tombstoned Entries never count, so
retract-and-redefine by one signer does not flag). The vocabulary view
SHALL carry both fields per documented family. `elephant log`
([[SPEC-001-elephant-core#REQ-016]]) SHALL annotate an Entry that
overwrites another signer's winning value for any [[#CON-401]] key with
the family name, the key, and the previous writer. Redefinition is
thereby visible, never prevented ([[#ADR-403]]).

Trace: [[#TEST-407]] · [[#ADR-403]] · [[#OBS-402]]

### Non-functional requirements

#### NFR-401: Vocabulary view latency

`elephant vocab` SHALL complete WITHIN the
[[SPEC-001-elephant-core#NFR-001]] closure budget + 50 ms at p95 on the
[[SPEC-001-elephant-core#NFR-003]] reference corpus. Family resolution,
classification, and docs join are linear in corpus + theory size
(provenance ([[#REQ-407]]) requires one pass over admitted Entries —
the merged metadata map alone carries no attribution).

Trace: [[#TEST-408]]

#### NFR-402: Advisory overhead

The [[#REQ-406]] advisory SHALL add zero closure evaluations to any
producer path: it is computed only when the command is served by a live
daemon ([[SPEC-002-elephant-p2p]]), from that daemon's current cached
view; a direct-store assert emits no advisory and its latency budget
([[SPEC-001-elephant-core#NFR-002]]) is untouched, as is producer
offline behaviour ([[SPEC-001-elephant-core#REQ-020]]). Reference-view
staleness is bounded by the daemon's change-watch refresh cycle.

Trace: [[#TEST-408]] · [[#TEST-406]]

## 3. Architecture decisions

#### ADR-401: The theory is its own ontology

**Decision.** elephant pins no vocabulary schema per theory. The
per-theory ontology *is* (a) the admitted rule set and commitment
triggers/goals — the signature: what is listened for, what is derivable
— and (b) label metadata on family atoms — the documentation. Both ride
ordinary signed speech acts. The new machinery is purely a *projection*
of the corpus (view, docs join, advisory), plus one validating producer
verb.

**Context.** Three candidate designs were examined. (1) A content-hashed
vocabulary object pinned at genesis, entries validated against it,
unknown predicates quarantined — the analogue of the pinned
`cbcl-elephant` dialect hash ([[SPEC-001-elephant-core#ADR-001]]).
(2) Vocabulary as a CBCL dialect extension, using R4 signing and R5
shape contracts. (3) Rules + meta as the emergent ontology, tooling for
discovery (this ADR). Design (1) solves an *integrity* problem this
project does not yet have, at the cost of the openness it does need: a
closed vocabulary chokes stigmergic discovery, and schema-valid names
(`ci-green` vs `build-ok`, both declared) still miss each other — a
schema cannot manufacture agreement, only reject strangers. Design (2)
is blocked architecturally: SPL rides as an opaque string inside CBCL
args (`src/core/envelope.rs`), so R5 shape contracts cannot see the
payload without splicing SPL into the CBCL grammar. Design (3) observes
that agreement is achieved at rule-authoring time and transmitted by the
existing pull channel (`why-not`/`require`) — the gap is discoverability
and documentation, not enforcement.

The evidence-rule convention (§1.3) concludes `verified-<task>` rather
than deriving `completed-<task>` because completion's given-fact
semantics are byte-frozen ([[SPEC-003-elephant-tasks#ADR-201]]); a
derived completion would change board semantics for every hence-ported
plan. `verified-` is already in the frozen discovery set, so the
convention composes with the freeze instead of amending it.

**Trade-offs.** (+) zero new corpus objects, performatives, or genesis
fields; openness preserved; documentation is attributable, syncable, and
retractable for free; the vocabulary can never drift from the theory
because it is derived from it. (−) no integrity guarantee: any member
can still assert junk (bounded, as today, by membership + trust
weighting, [[SPEC-001-elephant-core]] §8); agreement remains a
convention supported by tooling, not a theorem.

**Deferred — enforcement upgrade path.** If a deployment needs a closed
vocabulary (regulated domains, hostile-member models), the recorded
path is a genesis-declared `(vocab strict)` meta (precedent:
`(closure 2)`, [[SPEC-001-elephant-core#REQ-003]]) making fact-asserts
whose family has neither a listener nor documentation *quarantinable* at
merge — entry-local against the corpus prefix, version-gated exactly
like closure-2. Trigger for reopening: the first concrete deployment
where advisory-plus-trust-weighting demonstrably fails to contain
vocabulary abuse. Owner: HOC.

#### ADR-402: Families by structural suffix resolution over the frozen flat names

**Decision.** A [[Predicate Family]] is derived from a literal by
[[#CON-402]]: parameterised literals family on their predicate symbol;
flat atoms match the built-in registry patterns ([[#REQ-404]]), then
strip the longest declared-task suffix; everything else is its own
family. Resolution is purely structural — a function of (literal,
declared-task set) only, never of documentation — and documentation
attaches to the family atom.

**Context.** The frozen hence vocabulary hyphen-packs arguments into
atom names (`ci-green-m1`, `assign-to-m1-alice`), so the unit worth
documenting (the family) is not the unit that appears in conclusions
(the ground literal). The task layer already disambiguates hyphen-packed
names by longest-declared-task match (`src/tasks.rs` assignment
resolution — same discipline, applied there to the segment after
`assign-to-`, here to a trailing suffix); this spec reuses the
discipline rather than inventing a second convention. Entangling
resolution with documentation (docs-first precedence) was considered
and rejected: it would let any `(meta …)` writer re-partition the
family space, turning the documentation layer into a resolution attack
surface and making [[#CON-402]] non-local.

**Trade-offs.** (+) deterministic, total, locally computable;
documentation can never change what a literal means, only describe it.
(−) resolution depends on the declared-task set, so families can split
retroactively when a task is declared later — including adversarially
(declare task `green` → undocumented family `is-green` splits to `is`).
Accepted: the view is a re-derived projection, nothing stored depends
on it, and a documented family that stops resolving to itself is
surfaced as `detached` ([[#REQ-401]]) rather than silently dark; the
attack and its visibility are analysed in §8.

**Deferred.** The clean fix is parameterised literals —
`(ci-green m1)` — which spindle literals and the dag renderer already
support; the predicate symbol then *is* the family and this ADR's
resolver reduces to a lookup. That is a post-freeze vocabulary-v2
decision touching [[SPEC-003-elephant-tasks#ADR-201]]. Owner: HOC.

#### ADR-403: Documentation conflicts — per-key last write in canonical order, surfaced

**Decision.** Documentation converges **per key**: for each
(family, key), the winning value is the last write in the corpus's
canonical total order ((hlc, signer) —
[[SPEC-001-elephant-core#CON-002]]) among admitted, non-tombstoned
Entries. A family's current record is the merge of its per-key winners;
provenance and the `redefined` flag anchor on the `description` key
([[#REQ-407]]). Redefinition by a different signer is surfaced
([[#REQ-407]], [[#OBS-402]]), never blocked.

**Context.** This is not a new mechanism: the closure pipeline assembles
SPL in canonical order and spindle's `add_meta` overwrites per
(label, key) into one merged map per label
(`spindle-core/src/theory.rs`), so per-key last-write-wins is the
*existing, already-convergent* behaviour; this ADR pins it as contract
rather than accident — including its consequence that no single Entry
"owns" a record, which is why provenance needs a per-key anchor and a
raw-Entry scan ([[#NFR-401]]). Alternatives: first-write-wins lets a
fast peer squat names permanently with no corpus-level owner concept to
appeal to; making meta defeasible (trust-weighted docs) would feed
closure output into the assembly of closure's own metadata — a
stratification knot with no user demand behind it.

**Trade-offs.** (+) deterministic on every replica in any merge order
([[SPEC-001-elephant-core#REQ-021]] extends to the vocab view —
[[#TEST-408]]); retraction works (tombstoning a winning Entry revives
the previous write); per-key checking means one malformed key cannot
poison a record ([[#REQ-402]]). (−) any member can redefine any
family's docs, or flip its `kind` — accepted because documentation is
advisory ([[#ADR-404]]): the abuse is visible (journal annotation +
`redefined` flag), attributable, and weightless in the closure; the
kind-flip advisory-suppression vector is analysed in §8.

#### ADR-404: Advisory, never enforcement — and only from the daemon

**Decision.** The near-miss signal ([[#REQ-406]]) warns after a
successful append, is computed only from a live daemon's cached view,
and nothing in this spec blocks a producer, quarantines an Entry, or
alters closure on vocabulary grounds. Malformed documentation is
displayed as malformed ([[#REQ-402]]), not repaired and not quarantined.

**Context.** The trust boundary for *reasoning* is already fully policed
([[SPEC-001-elephant-core#REQ-022]]): every Entry is recognised in full
before it can influence closure. Vocabulary documentation influences
only display and advice — quarantining over it would spend the
fail-closed mechanism on a layer with no semantic authority, and
blocking asserts on unknown families would break the discovery
vocabulary's whole purpose and the offline-first producer contract
([[SPEC-001-elephant-core#REQ-020]]). Post-append (not pre-flight)
because the assert is *wanted* either way — the agent that meant a
different name retracts and re-asserts; the agent recording a discovery
proceeds untouched. Daemon-only because the alternative — running a
closure inside every direct-store assert — arithmetically breaks the
frozen producer latency budget ([[SPEC-001-elephant-core#NFR-002]]:
100 ms p95, vs a ~131 ms closure at the same corpus scale per
[[SPEC-001-elephant-core#ADR-013]]); the daemon already holds a fresh
view for its change watch, so the advisory there is a lookup, not a
computation.

**Trade-offs.** (+) zero new failure modes and zero added latency class
on the producer path; CI agents ([[users/agent/user]]) keep
fire-and-forget semantics with a machine-readable nudge in the receipt
— and the daemon is exactly where those agents already are (their
profile assumes it for cheap one-shot calls). (−) direct-store and
daemonless asserts get no advisory — accepted; the pull channel
([[#REQ-405]]) is the primary agreement loop, the advisory is the
safety net, and an agent that ignores its receipt learns nothing either
way. (−) the cached reference view can be momentarily stale, so an
advisory can be wrong in both directions — accepted and stated in
[[#REQ-406]]: it is advice. Enforcement remains the recorded upgrade
path in [[#ADR-401]].

#### ADR-405: Two flat verbs — `vocab` and `define`

**Decision.** One read verb `vocab` and one producer verb `define`, both
flat per [[SPEC-003-elephant-tasks#ADR-205]]; no `vocab` noun group.

**Context (capability placement, Constitutional Principle 15).** The
view could not live on an existing verb: `dag` renders the rule graph
per-literal (different projection, no docs), `describe` is per-label
lookup ([[SPEC-003-elephant-tasks#REQ-208]]), `status` is conclusions.
The producer could not be raw `assert`: the whole value of `define` is
full recognition against [[#CON-401]] *before signing*
([[PROTO-001]] LangSec), which `assert` must not do — arbitrary meta is
legal SPL. `describe` was rejected as the producer spelling (taken, and
read-vs-write puns on one verb violate one-spelling-per-action).

**Trade-offs.** (+) consistent with the flat surface; `define` reads as
the speech act it is. (−) two more leaves in `--help` (~26 total) —
absorbed by the existing declaration-order grouping.

## 4. Contracts

#### CON-401: Vocabulary documentation grammar

Input: the property set of an SPL `meta` directive attached to a family
atom — arriving via `define` argv (recognised in full before signing,
[[#REQ-403]]) or via the admitted corpus (recognised per key at view
time over the merged record, [[#REQ-402]]). Grammar (ABNF; on top of
SPL syntax per [[SPEC-001-elephant-core#CON-001]] — this layer
constrains *values*, SPL constrains form):

```abnf
vocab-doc   = desc-prop [kind-prop] [asserter-prop]   ; order-free in SPL
desc-prop   = "(" "description" SP quoted ")"  ; 1–512 bytes UTF-8,
                                               ; no C0/C1 controls
kind-prop   = "(" "kind" SP kind ")"
kind        = "evidence" / "state" / "discovery"
asserter-prop = "(" "asserter" SP quoted ")"   ; 1–128 bytes UTF-8,
                                               ; no C0/C1 controls
family      = alias                            ; the LDH label grammar of
                                               ; SPEC-001 REQ-003, verbatim
```

`family` additionally MUST NOT collide with the built-in registry
([[#REQ-404]]), and at `define` MUST resolve to itself under
[[#CON-402]] ([[#REQ-403]]). The 63-octet alias cap consequently caps
documentable family names — longer atoms remain legal SPL but
permanently undocumented (accepted; same cap rationale as theory
aliases). Kind `control` is not user-definable (built-in only). Pre:
family atom carries ≥ 1 property from the key set. Post (define): a
conforming doc record, or exit 3 naming the offending key/value. Post
(view): per-key conforming values, `malformed` markers for the rest —
never repair, never quarantine ([[#ADR-404]]). Keys outside the set are
out of scope (ignored here, preserved for other consumers; `source`
stays banned inline per [[SPEC-001-elephant-core#ADR-012]]).
Implements: [[#REQ-402]] [[#REQ-403]].
Verified by: [[#TEST-402]] [[#TEST-403]].

#### CON-402: Family resolution (pure core)

```
family(lit: &Literal, tasks: &BTreeSet<String>) -> String
```

Deterministic, total, no I/O, and a function of exactly these two
arguments (never of documentation — [[#ADR-402]]). Negation is ignored
(families are positive; occurrence *counting* elsewhere includes
negated positions, [[#REQ-401]]/[[#REQ-406]]). In order:

1. A literal with arguments → its predicate symbol.
2. A flat atom matching a built-in ground pattern of the [[#REQ-404]]
   table (the action/state/propagation sets there are closed — an
   unlisted `deploy-v3-m1` matches nothing here) → the table's family
   name.
3. A flat atom `<stem>-<task>` where `<task>` is the longest declared
   task that is a suffix preceded by `-` → `<stem>`.
4. Otherwise → the atom itself.

`tasks` is the declared-task set of the same closure pass the caller is
projecting — one resolver, one input, per evaluation.
Implements: [[#REQ-401]] [[#REQ-404]] [[#REQ-406]].
Verified by: [[#TEST-401]] [[#TEST-404]] [[#TEST-408]] (property:
total + deterministic under corpus shuffle).

#### CON-403: Vocabulary JSON contract

Extends [[SPEC-001-elephant-core#CON-004]] (v1 envelope discipline).
Ordering rule: `vocab` rows sorted by family, byte-lexicographic;
`holes`/candidate arrays sorted (task-suffix matches first, then
byte-lexicographic); object keys fixed as shown — determinism per
[[#TEST-408]] requires the ordering, not just the set.

```json
// vocab (array elements)
{ "family":"ci-green", "roles":["body"], "class":"hole",
  "built_in":false,
  "doc":{ "description":"CI pipeline green for the suffixed task",
          "kind":"evidence", "asserter":"role:ci",
          "documenter":"did:crdt:…", "redefined":false,
          "malformed":[], "detached":false } }
// assert receipt — advisory present only when REQ-406 fires
{ "v":1, "receipt":"s-…", "…":"…",
  "advisory":{ "kind":"inert-family",          // or "sibling"
               "family":"build-ok",
               "candidates":[ { "literal":"ci-green-m1",
                                "listener":"r-verified-m1" } ] } }
// why-not / require — added docs object (see REQ-405 for field scope)
{ "…":"…", "docs":{ "ci-green":{ "description":"…", "kind":"evidence",
                                  "asserter":"role:ci",
                                  "built_in":false } } }
```

All peer-controlled bytes rendered in text mode — descriptions,
asserters, family atoms, literals, rule labels — are printed with C0/C1
controls escaped, never raw (§8). Implements: [[#REQ-401]] [[#REQ-405]]
[[#REQ-406]] [[#REQ-407]]. Verified by: [[#TEST-401]] [[#TEST-405]]
[[#TEST-406]] ([[SPEC-001-elephant-core#TEST-019]] covers envelope
conformance).

## 5. Purity Boundary Map

Pure core (no I/O, no clocks): `core::vocab` — family resolution
([[#CON-402]]), the built-in registry (static data), role/class
computation, per-key docs conformance + LWW provenance ([[#ADR-403]],
one pass over admitted Entries), advisory computation ([[#REQ-406]]) —
all functions of `(ClosureResult, admitted entries, tasks)`. Effectful
shell: `cli` (new `vocab`/`define` arms, advisory rendering), `daemon`
(cached-view serving, [[#NFR-402]]). Boundary types: `VocabView`,
`FamilyDoc`, `Advisory`. Dependency rule unchanged: shell → core;
enforcement per [[SPEC-001-elephant-core]] §5 (clippy
`disallowed-methods`).

## 6. Test specification

Strategy per [[PROTO-001]]: pure-core projection logic → property +
example tests; the one new recogniser ([[#CON-401]]) is small and
value-level — example-tested at both entry points (argv, corpus bytes),
inheriting the existing SPL fuzz surface
([[SPEC-001-elephant-core#TEST-026]]) since it runs strictly after
`parse_spl`; AI-synthesised spec → requirement-targeted decomposition
and adversarial passes before `approved` (round 1 applied — see
Changelog).

| TEST | Validates | Positive | Negative-input | Negative-output |
|---|---|---|---|---|
| TEST-401 | REQ-401 | families listed with roles (incl. `goal` from commitments; negated body counts) /class/docs; hole = unproven body/goal instance, non-fact-head; orphan = unconsumed fact; detached doc flagged | per-key malformed → key marked, rest of record intact | discovery-kind or built-in fact flagged orphan → fail; documented family shown undocumented → fail; sibling-proven family masking an unproven instance's hole class → fail |
| TEST-402 | REQ-402 | `(meta fam …)` via raw assert recognised at view; tombstoned doc Entry reverts to prior winner per key | meta whose label names an admitted rule → rule metadata, not family doc; unknown keys ignored, preserved for `describe`; one bogus key does not undocument the family | vocab layer mutating or dropping foreign meta keys → fail |
| TEST-403 | REQ-403 | `define` → receipt; corpus+1; view shows doc | >512-byte desc, control chars, bad kind, malformed label, built-in collision, non-self-resolving family (`define ci-green-m1` with task m1 declared) → exit 3, corpus+0 | accepted define absent from view → fail |
| TEST-404 | REQ-404 | every table row resolves (`claim-v3-m1`→`claim`, `agent-alice-available`→`agent-available`, `no-deps-x`→`no-deps`); registry rows marked built-in with kinds | `define completed` / `define commitment-state` → exit 3; corpus meta on `completed` ignored | built-in family triggers advisory or orphan → fail; unlisted `deploy-v3-m1` treated as built-in → fail |
| TEST-405 | REQ-405 | why-not/require text + `--json docs` join corpus-documented and built-in families | undocumented families → output byte-identical to pre-405 shape | doc joined to wrong family → fail; `documenter`/`redefined` leaking into docs object → fail |
| TEST-406 | REQ-406 | daemon live: inert undocumented fact → inert-family advisory; `ci-green-m2` with only `ci-green-m1` listened → sibling advisory naming `ci-green-m1` + rule label; commitment-goal assert → NO advisory (goal role) | direct mode, `--no-advice`, rule payload, negated-body-consumed family, discovery-kind, built-in → no advisory | advisory changed exit code or blocked append → fail; advisory computation failure failing the assert → fail |
| TEST-407 | REQ-407 | second-signer conforming description → `redefined:true`, log annotates family+key+previous writer | same-signer update, or retract-then-redefine by one signer → not flagged; quarantined doc Entries never count | documenter disagrees with canonical per-key winner → fail |
| TEST-408 | NFR-401, NFR-402, [[SPEC-001-elephant-core#REQ-021]] extension | property: shuffled corpus ⇒ byte-identical `vocab --json` (ordering incl. per-key LWW winners); criterion bench within budget | — | divergence across merge orders → fail; any closure evaluation on a producer path → fail |

## 7. Observability

- OBS-401: advisory emissions logged (`tracing`: theory, advisory kind,
  family, candidate count) — the drift signal an operator tunes
  vocabulary against.
- OBS-402: redefinition events logged at view/journal time (family, key,
  previous → new writer) per [[#REQ-407]].
- `elephant daemon status --json` gains counters `advisories_emitted`,
  `vocab_views_served` (extends [[SPEC-001-elephant-core#OBS-002]]).

## 8. Security considerations

The closure trust boundary is untouched: no vocabulary datum influences
admission, conclusions, or commitment states ([[#ADR-404]]), so every
vector below misleads *readers and advice*, never the reasoner — and all
are attributable (signed Entries) and weightless in closure, bounded as
all member spam by roster membership and trust weighting
([[SPEC-001-elephant-core]] §8). Analysed vectors:

- **Terminal injection.** Descriptions, asserters, family atoms,
  literals, and rule labels are peer-controlled bytes reaching the
  operator's terminal. Conforming docs exclude C0/C1 controls by
  grammar ([[#CON-401]]); *all* peer-controlled bytes — conforming or
  not, docs or atoms — render with controls escaped, never raw
  ([[#CON-403]]).
- **Doc redefinition / kind-flip.** Any member can overwrite a family's
  winning keys ([[#ADR-403]]), including flipping `kind` to `discovery`
  to suppress the [[#REQ-406]] advisory and orphan flag for that
  family. Mitigation: per-key checking stops accidental/poisoned-key
  undocumenting ([[#REQ-402]]); every cross-signer overwrite is
  journalled with the key and previous writer ([[#REQ-407]],
  [[#OBS-402]]); `redefined` is visible in the view.
- **Family squatting via task declaration.** Declaring a task whose
  name suffixes existing atoms retroactively splits their families
  ([[#ADR-402]]), detaching documentation. Mitigation: resolution never
  depends on docs (no escalation), and detached documentation is
  surfaced per family ([[#REQ-401]] `detached`), so the squat is
  visible the moment any view runs.
- **Doc shadowing via rule label.** Asserting a rule *labeled* with a
  family atom reclassifies that atom's meta as rule metadata
  ([[#REQ-402]] precedence), undocumenting the family. Visible: the
  view shows the family undocumented while `describe` shows the
  colliding rule; attributable to the rule's signer.
- **Suggestion poisoning.** Advisory candidates are member-authored
  hole literals — a hostile member can plant rules with plausible body
  names to lure a mis-asserting agent into firing a hostile chain.
  Mitigation: every candidate carries its listening rule label / commit
  id ([[#REQ-406]], [[#CON-403]]) so provenance is one `describe` away;
  agents SHOULD treat candidates as leads, not commands. Residual risk
  accepted: the advisory shapes writes, but only toward literals that
  were already assertable.
- **Built-in shadowing.** Re-documenting frozen vocabulary
  (`completed`, `commitment-state`, …) to socially-engineer operators
  is closed: the registry ([[#REQ-404]]) is immutable from the wire and
  `define`-refused, and it includes [[SPEC-001-elephant-core#REQ-026]]'s
  reserved predicate, not only SPEC-003's freeze.

`define` recognises fully before signing; nothing is stored on rejection
([[PROTO-001]] LangSec). The advisory adds no producer failure modes
([[#REQ-406]] degradation clause) and no direct-store latency
([[#NFR-402]]).

## Changelog

<details>
<summary>Revision history — 0.1.0 → 0.2.0</summary>

- 0.2.0 (draft) — adversarial review round 1 (fresh-context,
  cross-session; 24 findings, 8 structural) applied. Structural fixes:
  commitment/request triggers and goals now count as listeners (role
  `goal`), closing the false advisory on the canonical fulfilment act
  ([[#REQ-401]], [[#REQ-406]]); built-in registry made closed and
  enumerable, audited against `src/tasks.rs` emissions, and extended
  with [[SPEC-001-elephant-core#REQ-026]]'s `commitment-state`
  ([[#REQ-404]]); [[#REQ-401]] row-criteria contradiction resolved
  (any documentation metadata admits a row); documentation conformance
  redefined per key over the merged record — one poisoned key no longer
  undocuments a family ([[#REQ-402]], [[#CON-401]], [[#ADR-403]]);
  provenance anchored on the `description` key with
  tombstone/quarantine scoping ([[#REQ-407]]); advisory made
  daemon-only against a defined reference view, resolving the latency
  contradiction with [[SPEC-001-elephant-core#NFR-002]] and the
  staleness incoherence ([[#REQ-406]], [[#NFR-402]], [[#ADR-404]]);
  hole granularity fixed to ground-instance level with family
  aggregation, and the sibling (wrong-suffix) advisory added —
  candidates now carry their listening rule label ([[#REQ-401]],
  [[#REQ-406]], [[#CON-403]]); task-suffix squatting and rule-label
  shadowing analysed, mitigated by structural-only resolution +
  `detached` surfacing + `define` reachability refusal ([[#ADR-402]],
  [[#REQ-401]], [[#REQ-403]], §8). Moderate fixes: rule-label
  precedence pinned; negated occurrences count as listeners; built-in
  docs join REQ-405 with field scope stated; JSON ordering pinned for
  determinism ([[#CON-403]], [[#TEST-408]]); NFR-401 architecture claim
  corrected (provenance needs an Entry pass); ADR-402 precedent wording
  corrected; escape-on-display widened to all peer-controlled bytes;
  suggestion poisoning and kind-flip suppression added to §8; hole
  definition uses "heads no non-fact rule". Comprehension gate
  (Orientation-only fresh reader): pass on intent, prediction, and
  artefact location. Still draft: awaiting adversarial round 2 and
  stakeholder validation.

- 0.1.0 — initial specification (Phase 1–2, draft). Derived from the
  stakeholder goal directive ("manage theory predicate vocab /
  ontology"), the design exploration of 2026-07-13 (agreement channels,
  rules-as-signature / meta-as-documentation, spindle `meta` carrier,
  LWW convergence analysis), [[SPEC-001-elephant-core]],
  [[SPEC-003-elephant-tasks]], and [[users/agent/happy-paths#HP-A1]].
  Pinned-schema and CBCL-R5 alternatives examined and recorded in
  [[#ADR-401]]; enforcement deferred with a named trigger.

</details>
