---
id: SPEC-005
title: elephant — predicate vocabulary: introspection, documentation, coining
version: 0.5.0
status: implementing
date: 2026-07-13
last-updated: 2026-07-14
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
[[#ADR-402]] a predicate family is spindle's
[[Predicate Symbol|`PredicateSymbol`]] `(functor, arity)` for a
parameterised literal (`(ci-green m1)` → `ci-green/1`), composed from
spindle's [[SPEC-024 predicate model|SPEC-024]] `TheorySignature`/
`Vocabulary`; else the flat atom's stripped longest-declared-task
suffix (legacy/frozen vocabulary). Parameterised literals are
RECOMMENDED for new vocabulary and are immune to the task-declaration
squat; flat-atom squatting is surfaced, never blocked ·
[[#ADR-403]] documentation conflicts converge by per-key
last-write-in-canonical-order, surfaced not prevented · [[#ADR-404]]
advisory, never enforcement — a near-miss warns, nothing blocks or
quarantines · [[#ADR-405]] two new flat verbs, `vocab` and `define`.

Load-bearing: [[#REQ-401]] vocabulary view · [[#REQ-402]] documentation
carrier · [[#REQ-404]] built-in registry · [[#REQ-406]] near-miss
advisory · [[#REQ-407]] redefinition provenance.

Open: opt-in genesis-declared vocabulary *enforcement* deferred with a
named trigger ([[#ADR-401]], owner HOC) · goal-driven demand for a
*single*-body template with no proven co-body witness is under-reported
by the [[#REQ-401]] witness join — surfacing it needs abduction the
cached view does not carry (owner HOC) · a `complete`-time advisory when
a `(verified ?t)` evidence rule exists but is unproven (touches
[[SPEC-003-elephant-tasks]] surface, owner HOC) ·
[[SPEC-024 predicate model|SPEC-024]] is itself `draft` — this spec
composes its *merged
implementation* (spindle `main`), so an API churn there is a tracked
risk, not a deferral ([[#ADR-402]], owner HOC).

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
- a **documentation carrier** — spindle's
  [[SPEC-024 predicate model|SPEC-024]] predicate metadata target (`(meta (predicate f n) …)`, or
  inline on a `(predicate …)` declaration) for declared predicates, and
  the legacy [[SPL]] `meta` label for flat vocabulary, under a declared
  value grammar ([[#REQ-402]]), with a validating producer verb `define`
  ([[#REQ-403]]);
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
is to document them in the same batch. Now that spindle carries
[[Parameterised Literal|parameterised literals]] and first-order
variables ([[#ADR-402]]), the RECOMMENDED form is a **single rule
quantified over the task variable** rather than one hyphen-packed rule
per task:

```lisp
; RECOMMENDED — one rule for every task, ?t bound by grounding;
; predicates DECLARED with arity + documented via the predicate meta-target
(predicate ci-green ((task symbol))
           (description "CI pipeline green for task ?t")
           (kind evidence) (asserter "role:ci"))
(predicate review-approved ((task symbol))
           (description "a human reviewer approved the change for ?t")
           (kind evidence) (asserter "role:reviewer"))
(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))
```

Here each family *is* its [[Predicate Symbol|`PredicateSymbol`]] —
`ci-green/1`, `review-approved/1`, `verified/1` — resolved by
[[#CON-402]] rule 1 from spindle's [[SPEC-024 predicate model|SPEC-024]]
`TheorySignature`, with no suffix heuristic; documentation attaches to
the **predicate symbol** via the [[SPEC-024 predicate model|SPEC-024]]
metadata target — inline on the `(predicate …)` declaration as shown, or
as a standalone `(meta (predicate ci-green 1) …)`. Keying on the
predicate symbol, not a label string, means a rule *labelled* `ci-green`
can no longer shadow the documentation (the §8 vector is closed for
declared predicates; [[#REQ-402]]). The legacy hyphen-packed form `(normally r-verified-m1
(and ci-green-m1 review-approved-m1) verified-m1)` with `(meta ci-green
…)` remains legal SPL and resolves by suffix ([[#CON-402]] rule 3), so
corpora predating predicate support still project.

Evidence rules SHOULD conclude `(verified ?t)` — family `verified/1`,
whose functor is part of the reserved discovery vocabulary — never `(completed
?t)` (or `completed-<task>`), whose given-fact semantics come from the
hence lifecycle pattern (recorded in [[SPEC-003-elephant-tasks#ADR-201]],
now superseded by [[SPEC-003-elephant-tasks#ADR-206]]; the predicate
stays reserved by [[#REQ-404]] registration; rationale in [[#ADR-401]]).
After this batch, every other participant reaches the names
mechanically: `require (verified m1)` returns the two evidence literals
*with their descriptions* ([[#REQ-405]]) — spindle's grounding
discriminates `(verified m1)` from `(verified m2)` at reasoning time, so
the abduced literals are exact ground instances, not suffix guesses;
nobody guesses.

## 2. Requirements

### Introspection (consumers — all local, no wire queries)

#### REQ-401: Vocabulary view

The CLI SHALL, on `elephant vocab -t <theory>`, run the closure pipeline
([[SPEC-001-elephant-core#CON-003]]) and print the theory's vocabulary as
one row per [[Predicate Family]] ([[#CON-402]]) that occurs in the
admitted theory (rule heads, rule bodies, facts, commitment/request
triggers and goals) or carries documentation metadata per [[#REQ-402]]
(conforming or not). For **parameterised families occurring in rules or
facts**, the row, its `head`/`body` roles, and its provenance are
**composed from spindle's `Vocabulary::derive`**
([[SPEC-024 predicate model|SPEC-024]]) — one entry per
[[Predicate Symbol|`PredicateSymbol`]], each carrying `OccurrenceRole`/
`PredicateOrigin` records. Elephant then (i) **re-groups** spindle's
arity-0 symbols for legacy flat atoms (`verified-m1/0`, `task-m1/0`, …)
into bare-stem families via [[#CON-402]] rules 2–4, and (ii)
**synthesizes** rows for families occurring only in a commitment/request
(the `goal` role) — which spindle's traversal never sees, since it has
no commitments. Each row shows:

- the family name and its **kind** discriminant ([[#CON-402]] `Family`):
  the predicate symbol `functor/arity` (kind `predicate`), the flat stem
  (kind `legacy`), or the escaped raw functor (kind `malformed`, rendered
  as its own row — never a silent drop); machine consumers key on
  `(kind, …)`, never on the rendered name alone ([[#CON-403]]);
- its **roles** — any of `fact` (heads a fact rule), `head` (heads a
  non-fact rule or defeater), `body` (occurs in a rule or defeater body,
  positively or negated), `goal` (occurs in a non-retracted, admitted
  commitment or request trigger or goal). `head`/`body` are read
  directly from spindle's `OccurrenceRole` (`Head`/`Body` + index + rule
  label); `fact` and `goal` are elephant-added — `fact` refines a `Head`
  occurrence by its rule's type (fact-rule vs non-fact rule/defeater),
  which `OccurrenceRole` does not distinguish, and `goal` covers
  commitments/requests, which spindle's model has no concept of;
- its **class** — `hole` (has role `body` or `goal`, heads no non-fact
  rule, and ≥ 1 of its *demanded ground instances* — as defined under
  **Grounding and demand** below — is not defeasibly provable), `orphan`
  (role `fact` only, is not built-in
  ([[#REQ-404]]) and carries no conforming documentation of kind
  `discovery`), else `active`;
- its **documentation** per [[#REQ-402]]: the per-key winning values,
  `malformed` markers naming each non-conforming key, provenance per
  [[#REQ-407]]; plus a `detached` marker on any documented family whose
  target matches **no** family projected from the admitted theory's
  literal occurrences under [[#CON-402]] — its documentation joins
  nothing (surfaced, never repaired). Occurrence-join, not
  bare-label-resolution: a functor with ≥ 1
  [[Parameterised Literal|parameterised]] occurrence is therefore never
  `detached` regardless of
  the declared-task set, while a flat stem whose atoms were all split
  away by a later task declaration ([[#ADR-402]]), a label no occurrence
  resolves to, or a nullary `Predicate(foo/0)` target (whose arity-0
  occurrences all resolve to the `Legacy` stem, [[#CON-402]]), is.

**Grounding and demand.** Roles are read at the family level from rule
and commitment structure: a variable body literal `(ci-green ?t)` gives
family `ci-green/1` role `body` directly, with no per-task enumeration.
The `hole`/`orphan` **classification** turns on a family's *demanded
ground instances*, defined below over a **supported fragment**; anything
outside that fragment contributes **no** demand and is classed `active`,
never `hole` (a bounded, named deferral — see *Out of fragment*).

*Proven fact (definition).* A ground literal is **proven** iff it is a
defeasibly-provable conclusion of the closure under the local trust
policy and evaluation time ([[SPEC-001-elephant-core#REQ-023]]) — i.e. it
appears in the closure's accepted weighted-conclusion set (admitted
status and net-positive weight), not merely materialised as an
intermediate derivation. "Provable"/"proven" throughout [[#REQ-401]] and
[[#REQ-406]] mean exactly this.

*Demanded ground instances (supported fragment).* Defined per occurrence
kind:

- **flat body/goal literal** → the ground atom itself, syntactic,
  present in the rule text (as before);
- **variable template all of whose variables are bound by the proven
  co-body witness join** → for a rule body binding a variable set `V`
  across sibling literals, each substitution `σ` over `V` such that
  **every other (sibling) body literal under `σ` is a proven fact**
  yields the demanded instance `template·σ`. This generalises the single
  shared `?t` to any number of variables — the join is over the sibling
  literals, so `(and (ci-green ?t) (assigned ?t ?a) (review-approved ?t))`
  demands `(ci-green c)` for each `(c, a)` with both `(assigned c a)` and
  `(review-approved c)` proven.

A demanded instance counts toward `hole`/sibling only when it is itself
**not** proven. This is a projection over the closure's already-grounded
facts (a join, no fresh query or abduction), so it holds the [[#NFR-401]]
budget and the [[#NFR-402]] no-extra-closure contract. The join is
*necessary* because the forward grounding fixpoint ([[Grounding]])
materialises only *provable* derivations — it never contains the
demanded-but-unprovable instance a hole is made of — so the demanded set
must be *computed*, not read off the fixpoint.

*Out of fragment (no demand → `active`, deferred; owner HOC).* The
following occurrences contribute no demanded instance and never make a
`hole`, deliberately under-reporting until the [[#ADR-402]] SPEC-024
`vocabulary`-module composition (which derives occurrence demand
natively):

- a **template with a variable not bound by any proven co-body witness**
  (unbound or partially-bound) — no witness, no ground demand;
- a **ground parameterised goal with no proven co-body binder** — the
  single-body / goal-only case (`(verified m1)` wanted, nothing proven to
  bind it); surfacing this goal-driven demand needs abduction the cached
  view does not carry;
- **arithmetic argument positions** — spindle retains them in the arity
  ([[SPEC-024 predicate model|SPEC-024]] REQ-002) but they are not
  ground-joinable against proven facts here;
- **temporal variables / temporally-bound occurrences** — the join is
  over logical siblings, not the temporal dimension;
- **`Malformed(_)` occurrences** — no well-formed instance to demand.

Flat vocabulary, whose demand is the syntactic body atom, classifies
exactly as before and sits wholly inside the fragment ([[#ADR-402]]).

Like every consumer command, the view is evaluated under the local trust
policy and evaluation time ([[SPEC-001-elephant-core#REQ-023]]): rows
derive from the corpus alone; `hole`/`active` classification may differ
across replicas exactly where their trust policies differ. WITH `--json`
the output SHALL follow [[#CON-403]], including its ordering rule.

Trace: [[#TEST-401]] · [[#CON-402]] · [[#CON-403]]

#### REQ-402: Documentation carrier

The vocabulary layer SHALL recognise as a parameterised family's
documentation the metadata carried on its
[[Predicate Symbol|`PredicateSymbol`]] via spindle's
**`MetaTarget::Predicate`** carrier ([[SPEC-024 predicate model|SPEC-024]])
— authored inline on a `(predicate ci-green ((task symbol)) …)`
declaration or as a standalone `(meta (predicate ci-green 1) …)`, both
of which spindle stores against the symbol `ci-green/1`. Because the key
is the predicate symbol, not a label string, a rule *labelled* `ci-green`
can no longer shadow the documentation (the §8 vector is closed for
declared predicates — see [[#REQ-403]] and §8). For **legacy** flat families the layer SHALL also
read the label-string `(meta <stem> …)` carrier (spindle's `add_meta`
under `MetaTarget::Label`), whose rule-label precedence remains
load-bearing for those atoms. Documentation is evaluated **per key over
the merged record** of the admitted theory as
assembled by the closure pipeline: for each key in the [[#CON-401]] set
(`description`, `kind`, `asserter`), the winning value ([[#ADR-403]]) is
checked against [[#CON-401]] independently; a non-conforming key is
surfaced as `malformed` and treated as absent, never repaired and never
poisoning the other keys. A family is **documented** when its
`description` key is present and conforming. Documentation rides
ordinary signed `assert` Entries — it syncs, tombstones (E1), and
quarantines like any other statement; no new corpus object, performative,
or genesis field.

Precedence (legacy label carrier only): a *label-targeted* `meta` whose
label names an admitted rule or defeater is rule metadata, not family
documentation — the vocabulary layer SHALL ignore it (rule labels win;
see §8). This precedence does **not** apply to `MetaTarget::Predicate`
documentation, which targets the symbol and cannot collide with a label.
Meta keys outside the [[#CON-401]] set, and metadata on non-family
targets, SHALL be ignored by the vocabulary layer and left untouched for
their existing consumers ([[SPEC-003-elephant-tasks#REQ-208]] `describe`,
task declarations).

Trace: [[#TEST-402]] · [[#CON-401]] · [[#ADR-401]] · [[#ADR-403]]

#### REQ-403: Define (producer)

The CLI SHALL, on `elephant define <functor>/<arity> --desc <text>
[--kind evidence|state|discovery] [--asserter <who>] [--arg
<name>:<sort>]… -t <theory>`, fully recognise the arguments against
[[#CON-401]] — the [[SPEC-024 predicate model|SPEC-024]] predicate
indicator `functor/arity` (with `arity ≥ 1` and no leading-zero or
overflowing arity, [[#CON-401]]), description and asserter caps, kind
enumeration, and (when `--arg` is given) argument count equal to
`arity` with unique names and primitive sorts — and refuse (exit 3) a
`functor/0` indicator (a nullary predicate is not a `define` target —
[[#CON-402]]) or a symbol whose functor collides with the built-in
registry ([[#REQ-404]]).
Because the target is an arity-bearing
[[Predicate Symbol|`PredicateSymbol`]], `define` fully recognises before
signing ([[PROTO-001]] LangSec) — the 0.3.0 functor-vs-flat-stem
ambiguity is gone: `ci-green/1` is unambiguously a predicate symbol,
never a hyphen-packed instance, so no reachability warning is needed.
Rejection SHALL occur before signing or storing anything; on success
`define` SHALL append one signed `assert` Entry whose payload is a
`(predicate <functor> ((<name> <sort>)…) …)` declaration when `--arg`
is supplied, else a bare `(meta (predicate <functor> <arity>) …)`
meta-target, and print the sentence-id as receipt. Raw `elephant assert
'(predicate …)'` / `'(meta (predicate …) …)'` remains legal and
unvalidated against [[#CON-401]] (ordinary SPL per
[[SPEC-001-elephant-core#CON-001]]); `define` is the recognising
producer — the one place vocabulary documentation is checked at
authoring time. Documenting a **legacy** flat stem (no arity) is not a
`define` target; it stays a raw `(meta <stem> …)` assert.

Trace: [[#TEST-403]] · [[#CON-401]] · [[SPEC-001-elephant-core#REQ-005]]

#### REQ-404: Built-in vocabulary registry

The vocabulary layer SHALL ship the following closed built-in registry —
the hence lifecycle vocabulary of [[SPEC-003-elephant-tasks]] §1 (still
*reserved* here after the task verbs that emitted it were removed in
[[SPEC-003-elephant-tasks#ADR-206]], so legacy corpora remain
interpretable) plus [[SPEC-001-elephant-core]]'s reserved predicate. This
table is normative and exhaustive; membership is not extensible from the
wire.

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
| `failed` | `failed-<task>` | control |
| `discovered` `decided` `blocked-by` `requires` `verified` `finding` `approach` `insight` `partial` | `<family>-<suffix>` | discovery |

(`failed` appears in both SPEC-003 lists; it is registered **once**, in
its own row above with kind `control` — not in the discovery row — because
propagation rules consume it; the earlier draft's discovery-row placement
contradicted this prose and is corrected here.) **Membership is by
functor.** Each row reserves its functor at **every arity** *and* its
legacy flat suffix family: `verified` reserves `verified/0`,
`verified/1` (`(verified m1)`), `verified/2`, … *and* the flat stem
`verified` (`verified-m1`); `commitment-state` reserves
`commitment-state/N` for all N (canonically `/2`,
[[SPEC-001-elephant-core#REQ-026]]). Reserving alternate arities of a
built-in functor is a deliberate over-reservation (accepted: a reserved
discovery/control name cannot be shadowed at *any* arity; the space of
useful new vocabulary is unaffected).

Each built-in family carries the following **fixed** `description` — the
text the [[#REQ-401]] view and [[#REQ-405]] join surface. Built-in
families carry no `asserter`; their provenance is `documenter: null`,
`redefined: false` ([[#CON-403]]):

| Family | Fixed description |
|---|---|
| `task` | declared unit of work (legacy hence lifecycle) |
| `no-deps` | task has no dependencies (readiness root) |
| `ready` | task's dependencies are met; it may be claimed |
| `completed` | task is finished (given fact; lifecycle terminal) |
| `claimed` | an agent has taken ownership of the task |
| `blocked` | task is blocked by a recorded impediment |
| `upstream-blocked` | a dependency of the task is blocked |
| `decomposed` | task was split into sub-tasks |
| `permanently-failed` | task failed with no retry path |
| `assign-to` | task is assigned to the named agent |
| `agent-available` | the named agent is available for assignment |
| `claim` `unclaim` `block` `unblock` | versioned lifecycle action on the task |
| `state-claimed` `state-unclaimed` `state-blocked` `state-unblocked` | versioned lifecycle state of the task |
| `stale` `timeout` | versioned staleness/timeout signal on the task |
| `commitment-state` | current state of a commitment ([[SPEC-001-elephant-core#REQ-026]]) |
| `failed` | work failed (evidence signal; consumed by propagation rules) |
| `discovered` | a fact an agent discovered during work |
| `decided` | a decision an agent recorded |
| `blocked-by` | records what blocks progress |
| `requires` | records a prerequisite an agent found |
| `verified` | evidence a task's work is verified (RECOMMENDED evidence-rule conclusion) |
| `finding` | a finding recorded during work |
| `approach` | an approach an agent recorded |
| `insight` | an insight recorded during work |
| `partial` | a partial result recorded during work |

Built-in entries carry these fixed descriptions, appear in [[#REQ-401]]
output marked `built-in`, SHALL never be classed `orphan` or trigger the
[[#REQ-406]] advisory, and SHALL NOT be definable: `define` of any symbol whose
**functor** is a built-in SHALL be refused (exit 3) — `define
verified/2` as much as `define verified/1` — and corpus predicate-meta
*or* legacy label-meta on a built-in functor (any arity) or stem SHALL
be ignored by the vocabulary layer (the freeze is not overridable from
the wire).

Trace: [[#TEST-404]] · [[SPEC-003-elephant-tasks#ADR-201]] · [[#CON-402]]

#### REQ-405: Documented explanations

`elephant why-not <literal>` and `elephant require <literal>` SHALL join
each missing/abduced literal against the documentation of its family —
corpus documentation ([[#REQ-402]]) and built-in registry entries
([[#REQ-404]]) alike: in text output, a documented family's description
follows the literal; WITH `--json`, the existing shapes gain a `docs`
object mapping family → `{family_kind, description, kind, asserter?,
built_in}` (the `family_kind` discriminant so predicate/legacy keys never
collide — [[#CON-403]]) for
every documented family appearing in `missing`/`solutions`
(undocumented families omitted; `documenter`/`redefined` deliberately
omitted here — provenance lives in the vocab view; shape per
[[#CON-403]]). Output for undocumented families is unchanged. The
missing/abduced literals are whatever spindle's `why-not`/`require`
surfaces return — now exact ground instances that discriminate `(p a)`
from `(p b)` — so the family join keys on their predicate symbol
`functor/arity` ([[#CON-402]] rule 1) with no suffix inference for
parameterised vocabulary.

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
  (roles read at the family level per [[#REQ-401]] grounding — a
  variable body template `(ci-green ?t)` gives `ci-green` role `body`;
  negated occurrences count, a negatively-consumed fact is not inert),
  (b) is not built-in ([[#REQ-404]]), and (c) is not documented with
  kind `discovery`; or
- a **sibling advisory** when the family does have role `body` or
  `goal` but the asserted ground literal is itself not a demanded
  instance ([[#REQ-401]] demand) in the reference view, while ≥ 1
  sibling instance of the same family is an unproven demanded instance —
  the wrong-argument case, whether spelled `(ci-green m2)` vs a
  demanded-but-unproven `(ci-green m1)` or, in legacy flat form,
  `ci-green-m2` vs `ci-green-m1`. For a variable template `(ci-green
  ?t)`, the sibling `(ci-green m1)` counts only when a proven co-body
  binds `?t = m1` per the [[#REQ-401]] witness join (else there is no
  live demand and no advisory); the near-miss it catches is exactly "you
  asserted `(ci-green m2)`, but the live demand is for `(ci-green m1)`".

Either advisory SHALL list at most 5 candidate ground literals drawn from
the unproven body/goal occurrences in the reference view. For a
**sibling** advisory, the same-family demanded-but-unproven sibling(s)
that triggered it SHALL be included and ranked **first**, ahead of every
other candidate — the cap MUST NOT drop the sibling the advisory names
(the canonical wrong-argument case's expected sibling has, by
construction, a *different* argument from the asserted fact, so an
argument-first ranking would otherwise let five unrelated
argument-sharing holes hide it — [[#TEST-406]]). The remaining slots (up
to 5 total) fill with the other unproven occurrences, those sharing the
fact's resolved argument first (the trailing task for a flat atom, the
matching term for a parameterised one), then byte-lexicographically by
literal, then by listener label ([[#CON-403]] ordering). Each candidate
carries the label of the rule (or the sentence-id of the commitment) that
listens for it, so candidates are attributable (§8).

The advisory travels on the daemon append path per the amended
[[SPEC-002-elephant-p2p#CON-101]] control contract: the append request
carries an `advice` control (enabled by default; `--no-advice` sets it
`false`), the daemon evaluates the fact against the reference view
**before** applying the append, and returns the `advisory` object in the
append response receipt ([[#CON-403]]). Text mode prints it to stderr;
`--json` surfaces the same object. The advisory SHALL NOT
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
classification, and docs join are linear in corpus + theory size;
parameterised-template `hole` classification adds the proven co-body
witness join ([[#REQ-401]]), a projection over the closure's
already-grounded facts (bounded by grounded-fact count × rule body
arity, no fresh closure or abduction pass). Provenance ([[#REQ-407]])
requires one pass over admitted Entries — the merged metadata map alone
carries no attribution.

Trace: [[#TEST-408]]

#### NFR-402: Advisory overhead

The [[#REQ-406]] advisory SHALL add zero closure evaluations to any
producer path: it is computed only when the command is served by a live
daemon ([[SPEC-002-elephant-p2p]]), from that daemon's current cached
view; a direct-store assert emits no advisory and its latency budget
([[SPEC-001-elephant-core#NFR-002]]) is untouched, as is producer
offline behaviour ([[SPEC-001-elephant-core#REQ-020]]).

**Reference view — initialization and refresh.** The reference view is
the daemon's existing cached closure — the one it already maintains for
its change watch ([[SPEC-002-elephant-p2p#CON-103]] live phase, recomputed
once per merged batch). It is **not** a structure this spec adds: the
[[#REQ-406]] evaluation is a read of that cache taken *before* the append
is applied, so it reflects exactly the entries preceding this append and
adds no closure pass. It is **refreshed** on the same change-watch cycle
(each `subscribe_local_update` batch), which bounds its staleness. On a
cold daemon whose cache is not yet warm for the target theory, the append
proceeds and no advisory is emitted (the [[#REQ-406]] degradation clause),
never a blocked or delayed assert.

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

The evidence-rule convention (§1.3) concludes `verified` — `(verified
?t)`, or `verified-<task>` in legacy flat form — rather than deriving
`completed` because completion carries given-fact semantics in the hence
lifecycle pattern ([[SPEC-003-elephant-tasks#ADR-201]], superseded by
[[SPEC-003-elephant-tasks#ADR-206]] but still the shape of any legacy
corpus); a derived completion would change those semantics wherever such
a plan is present. The `verified` functor is already in the reserved
discovery set, so the convention composes with it instead of amending
it — though the two spellings resolve to **distinct family keys**
(`(verified ?t)` → `verified/1`, `verified-<task>` → `verified`,
[[#CON-402]]); both are reserved built-ins ([[#REQ-404]] reserves the
functor at every arity), so an agent reaches the descriptions either
way. A corpus mixing the spellings shows both rows — accepted, the view
is a projection ([[#REQ-401]], [[#ADR-402]]).

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

#### ADR-402: Families as spindle PredicateSymbols, with suffix resolution for the frozen flat names

**Decision.** A [[Predicate Family]] is derived from a literal by
[[#CON-402]]. A [[Parameterised Literal|parameterised literal]] families
on its [[Predicate Symbol|`PredicateSymbol`]] `(functor, arity)` —
**composed from spindle's [[SPEC-024 predicate model|SPEC-024]]
`TheorySignature`/`Vocabulary`** (`Literal::predicate_symbol()`,
`Vocabulary::derive`), not a hand-rolled resolver (Simplicity Ladder
rung 4). This is the RECOMMENDED shape for all new vocabulary. A flat
atom instead matches the built-in registry patterns ([[#REQ-404]]), then
strips the longest declared-task suffix; everything else is its own
family — the legacy path, kept in elephant because the frozen hence
vocabulary is permanently hyphen-packed and spindle's model has no
notion of it. Resolution is purely structural — a function of (literal,
declared-task set) only, never of documentation — and documentation
attaches to the **predicate symbol** via SPEC-024's `MetaTarget::Predicate`
([[#REQ-402]]), not to a punned label string.

**Context.** The frozen hence vocabulary hyphen-packs arguments into
atom names (`ci-green-m1`, `assign-to-m1-alice`), so the unit worth
documenting (the family) is not the unit that appears in conclusions
(the ground literal). The retired hence task layer disambiguated
hyphen-packed names by longest-declared-task match — applied there to the
segment after `assign-to-`, here to a trailing suffix. With the task
verbs removed ([[SPEC-003-elephant-tasks#ADR-206]]) that resolver is gone,
so this spec carries the *discipline* forward directly over the closure's
provable-`task-X` set ([[#CON-402]] `tasks`) rather than inventing a
second convention. Entangling
resolution with documentation (docs-first precedence) was considered
and rejected: it would let any `(meta …)` writer re-partition the
family space, turning the documentation layer into a resolution attack
surface and making [[#CON-402]] non-local.

**Why compose rather than reimplement.** SPEC-024 (merged on
spindle `main`, PR #34) already models exactly the unit this spec needs
to document: a `PredicateSymbol = (functor, arity)`, a `TheorySignature`
(the symbol set, union of occurrences and declarations), a `Vocabulary`
with per-symbol descriptions and `OccurrenceRole`/`PredicateOrigin`
provenance, and a `MetaTarget::Predicate` carrier that attaches metadata
to a symbol without punning a label. Reimplementing that in elephant
would duplicate a merged, reviewed, Lean-anchored model — a Simplicity-
Ladder rung-4 violation and a second parser for the same language
([[PROTO-001]] LangSec, one-parser-per-language). Elephant therefore
**composes** it and adds only what is elephant-specific: the `goal` role
(commitments/requests, which spindle has no concept of), the legacy
suffix resolver for the frozen flat vocabulary, and the `hole`/demand
computation — because SPEC-024 explicitly *does not touch inference*
(its `Vocabulary` is "not a source of reasoning truth"), so unprovability
(the substance of a hole) stays elephant's, via the [[#REQ-401]]
witness join.

The evidence-rule convention above still concludes `verified`, not
`completed`, for the reason in [[#ADR-401]].

**Trade-offs.** (+) family identity, roles, provenance, and the
documentation carrier are a merged upstream model, not elephant code;
`(functor, arity)` is exact and `tasks`-independent, so a parameterised
family cannot be split by a task declaration; documenting on the
predicate symbol closes the rule-label shadow (§8) outright. (−) the
**suffix** rules still depend on the declared-task set, so *flat-atom*
families can split retroactively when a task is declared later —
including adversarially (declare task `green` → undocumented flat family
`is-green` splits to `is`). This hazard is now confined to the legacy
path; the structural fix is to author new vocabulary as declared
predicates. For the flat atoms that remain it is accepted — the view is
a re-derived projection, nothing stored depends on it, and a documented
family joining no occurrence is surfaced as `detached` ([[#REQ-401]]);
the attack and its visibility are analysed in §8. (−) elephant now takes
a hard dependency on spindle's SPEC-024 surface, itself `draft` — a
tracked API-churn risk (Orientation `Open`), bounded by the compat
wrappers SPEC-024 kept (`add_meta`/`get_meta`).

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
SPL in canonical order and spindle overwrites metadata per
(`MetaTarget`, key) into one merged map per target — label *or*
predicate symbol ([[SPEC-024 predicate model|SPEC-024]];
`spindle-core/src/theory.rs`) — so per-key last-write-wins is the
*existing, already-convergent* behaviour for both carriers; this ADR
pins it as contract rather than accident — including its consequence
that no single Entry "owns" a record, which is why provenance needs a
per-key anchor and a raw-Entry scan ([[#NFR-401]]). Alternatives: first-write-wins lets a
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

Input: the documentation properties carried on a
[[Predicate Symbol|`PredicateSymbol`]] — via a `(predicate …)`
declaration or a `(meta (predicate functor arity) …)` meta-target —
arriving via `define` argv (recognised in full before signing,
[[#REQ-403]]) or via the admitted corpus (recognised per key at view
time over the merged record, [[#REQ-402]]). The predicate-declaration
and predicate-indicator *forms* are spindle's
([[SPEC-024 predicate model|SPEC-024]] CON/grammar, recognised by
`spindle_parser` — one
parser per language, [[PROTO-001]]); this layer constrains only the
documentation *values* on top of them (ABNF):

```abnf
; SPL property lists are order-free s-expressions; each key occurs at
; most once in a well-formed target. A repeated key is NOT a grammar
; error — it is resolved by per-key LWW over the merged record
; ([[#ADR-403]]). `description` is NOT syntactically required: kind-only
; and asserter-only targets are well-formed and are processed per key
; ([[#REQ-402]] evaluates each key independently). A family being
; *documented* — a conforming `description` present — is a semantic
; classification layered on top of this grammar, not a production of it.
vocab-doc      = 1*prop            ; ≥ 1 property (Pre); each key ≤ once, any order
prop           = desc-prop / kind-prop / asserter-prop
desc-prop      = "(" "description" SP quoted ")"  ; value 1–512 bytes UTF-8,
                                                  ; no C0/C1 controls
kind-prop      = "(" "kind" SP kind ")"
kind           = "evidence" / "state" / "discovery"
asserter-prop  = "(" "asserter" SP quoted ")"     ; value 1–128 bytes UTF-8,
                                                  ; no C0/C1 controls
pred-indicator = functor "/" arity   ; SPEC-024 REQ-003, e.g. ci-green/1
functor        = alias               ; LDH label grammar, SPEC-001 REQ-003
arity          = "0" / ( nzdigit *DIGIT )  ; canonical, no leading zeros;
nzdigit        = %x31-39                    ;   numeric value ≤ 2^32−1 (u32,
                                            ;   SPEC-024 CON-002 / ArityOverflow)
```

`functor` additionally MUST NOT be a built-in functor at *any* arity
([[#REQ-404]]; registry membership is functor-level — every arity of a
reserved functor is reserved, so the collision test ignores `arity`);
when a full declaration is authored, the argument count MUST equal
`arity` with unique names and primitive sorts (`symbol`/`integer`/
`decimal`/`float`/`number`/`any`, [[SPEC-024 predicate model|SPEC-024]]).
For a `define` target the `arity` MUST additionally be ≥ 1 ([[#CON-402]]
nullary refusal); a leading-zero spelling (`01`) or a value exceeding
`2^32−1` is malformed — rejected at `define` (exit 3), and at view time
spindle's `PredicateSymbol::try_new` yields `ArityOverflow`, so the
occurrence resolves to a `Malformed(_)` family ([[#CON-402]]), never a
silent wrap. The 63-octet alias cap caps functor names — longer remain
legal SPL but permanently undocumentable (accepted; theory-alias cap
rationale). Kind `control` is not user-definable (built-in only). Pre:
the target carries ≥ 1 property from the key set. Post (define): a
conforming doc record, or exit 3 naming the offending key/value/arity.
Post (view): per-key conforming values, `malformed` markers for the rest
— never repair, never quarantine ([[#ADR-404]]). Keys outside the set are out of scope
(ignored here, preserved for other consumers; `source` stays banned
inline per [[SPEC-001-elephant-core#ADR-012]]). The legacy flat-stem
label form (`(meta <stem> …)`) is still read at view time ([[#REQ-402]])
but is not a `define` target. Implements: [[#REQ-402]] [[#REQ-403]].
Verified by: [[#TEST-402]] [[#TEST-403]].

#### CON-402: Family resolution (pure core)

```rust
enum Family {
    // parameterised / declared predicate — spindle (functor, arity), arity ≥ 1
    Predicate(PredicateSymbol),
    // legacy flat vocabulary — a hyphen-packed or bare arity-0 atom stem
    Legacy(String),
    // functor spindle cannot form a PredicateSymbol from (empty / control char);
    // carries the raw functor bytes (escaped on display), so two distinct
    // malformed atoms stay distinct families — never merged under one sentinel
    Malformed(String),
}

family(lit: &Literal, tasks: &BTreeSet<String>) -> Family
```

`Family` is a **discriminated identity**, never a bare `String`. Its three
constructors occupy disjoint key spaces even when they render to the same
bytes, closing the 0.4.0 collision where a `String` key conflated them:

- a parameterised `(p x)` families to `Predicate(p/1)`;
- a *flat atom literally spelled* `p/1` — legal SPL, since `/`, `<`, `>`
  are atom characters ([[SPL]] lexer `parse_atom`) — families to
  `Legacy("p/1")`, a **distinct** family that never merges with
  `Predicate(p/1)`;
- likewise the malformed variant never collides with a flat atom spelled
  `<malformed>`, which is `Legacy("<malformed>")`; and two literals with
  *different* control-char functors yield *different* `Malformed(_)`
  families, so unrelated malformed atoms never share roles, docs, or a
  JSON map entry.

[[#CON-403]] carries the discriminant on the wire as `kind`
(`predicate` / `legacy` / `malformed`); machine consumers key on the
`(kind, …)` pair, never on the rendered `family` string alone.

Deterministic, total, no I/O, and a function of exactly these two
arguments (never of documentation — [[#ADR-402]]). Totality holds
because every branch, including rule 1's fallible call (its `Err` case
below), maps to a defined `Family`. Negation is ignored (families are
positive; occurrence *counting* elsewhere includes negated positions,
[[#REQ-401]]/[[#REQ-406]]). In order:

1. A literal with **≥ 1 argument** whose functor forms a valid
   [[Predicate Symbol|`PredicateSymbol`]] → `Predicate(functor/arity)`,
   obtained from spindle's `Literal::predicate_symbol()`
   ([[SPEC-024 predicate model|SPEC-024]]; `(ci-green m1)` →
   `Predicate(ci-green/1)`, `(commitment-state <id> <state>)` →
   `Predicate(commitment-state/2)`). This is the **primary, RECOMMENDED
   path**: identity is spindle's, exact, independent of `tasks`, and
   therefore immune to the task-declaration family-split of [[#ADR-402]].
   **Arity 0 never takes this rule** — a bare atom (`verified-m1`, `foo`)
   has no arguments and falls to the legacy rules below (see *Nullary
   declared predicates*). If `predicate_symbol()` returns `Err` (empty or
   control-character functor — normally already rejected at
   [[#CON-401]]/SPL parse), `family` SHALL return `Malformed(<raw
   functor>)`, which [[#REQ-401]] renders as a `malformed` occurrence
   (never a silent drop, never a panic), aligning with spindle's own
   `MalformedPredicate` diagnostic.
2. A flat atom matching a built-in ground pattern of the [[#REQ-404]]
   table (the action/state/propagation sets there are closed — an
   unlisted `deploy-v3-m1` matches nothing here) → `Legacy(<table
   family>)`.
3. A flat atom `<stem>-<task>` where `<task>` is the longest member of
   `tasks` that is a suffix preceded by `-` → `Legacy(<stem>)`.
4. Otherwise → `Legacy(<atom>)`.

**The `tasks` argument.** `tasks` is the set of task names X such that
the fact `task-X` — the [[SPEC-003-elephant-tasks]] §1 declaration form
`(given task-X)` — is an **admitted, defeasibly-provable fact** in the
same closure pass the caller is projecting, under the local trust policy
and evaluation time ([[SPEC-001-elephant-core#REQ-023]]). It is a pure
projection of the corpus: **not** the rule-head set, **not** raw `given`
Entries irrespective of trust or tombstoning, and **not** commitment
goals. With the hence task verbs retired
([[SPEC-003-elephant-tasks#ADR-206]]) no live task layer supplies this
set — `task-X` facts now survive only in legacy corpora — so the
vocabulary layer derives `tasks` from the closure's provable-`task-X`
conclusions, applying the same longest-declared-task discipline the
retired assignment resolver used. The legacy rules 2–4 consult `tasks`;
rule 1 never does, so a parameterised family cannot be split by a task
declaration ([[#ADR-402]]).

**Nullary declared predicates (`functor/0`).** Because rule 1 requires
arity ≥ 1, an arity-0 occurrence always resolves to a `Legacy` family;
consequently a `Predicate(foo/0)` documentation target joins no
occurrence and is always surfaced `detached` ([[#REQ-401]]), never
repaired. `define` therefore SHALL refuse a `functor/0` indicator
(exit 3, [[#REQ-403]], [[#CON-401]]): a nullary predicate is not a
`define` target and is documented, if at all, as a legacy
`(meta <stem> …)` on its bare atom. Raw `assert '(meta (predicate foo
0) …)'` stays legal SPL but its target shows `detached` in the view.

Rule 1 is a thin wrapper over spindle's predicate-symbol projection;
rules 2–4 are the **legacy path** elephant keeps for the frozen
hyphen-packed vocabulary ([[SPEC-003-elephant-tasks]] §1) and any
flat-atom (arity-0) corpus predating predicate support, since spindle's
model has no notion of the hence naming convention. A parameterised
literal never reaches them. New vocabulary SHOULD be declared predicates
so it families on its symbol alone (rule 1); the resolver stays hybrid
only because the frozen vocabulary is permanently hyphen-packed and stays
interpretable ([[#REQ-404]]).
Implements: [[#REQ-401]] [[#REQ-404]] [[#REQ-406]].
Verified by: [[#TEST-401]] [[#TEST-404]] [[#TEST-408]] (property:
total + deterministic under corpus shuffle).

#### CON-403: Vocabulary JSON contract

Extends [[SPEC-001-elephant-core#CON-004]] (v1 envelope discipline).
Object keys fixed as shown — determinism per [[#TEST-408]] requires the
ordering, not just the set.

**Discriminant.** Every family-bearing object carries a `kind`
∈ {`predicate`, `legacy`, `malformed`} — the [[#CON-402]] `Family`
discriminant. `family` is the rendered key (the indicator `functor/arity`
for `predicate`, the flat stem for `legacy`, the escaped raw functor for
`malformed`), and machine consumers MUST key on the `(kind, family)`
pair, never on `family` alone: a parameterised `(p x)` (`kind:"predicate",
family:"p/1"`) and a flat atom spelled `p/1` (`kind:"legacy",
family:"p/1"`) are distinct rows. `arity` is broken out **only when**
`kind == "predicate"` (an integer ≥ 1); it is absent for `legacy` and
`malformed`. `functor` is present for `predicate` (the bare functor) and
absent otherwise. `literal` carries the ground literal in whatever
spelling the corpus uses — parameterised `(ci-green m1)` or legacy flat
`ci-green-m1`.

**Ordering (all arrays; required for [[#TEST-408]]).**

- `vocab` rows: by `(kind, family)` — `kind` in the fixed order
  `predicate` < `legacy` < `malformed`, then `family` byte-lexicographic
  (so predicate `p/1` and legacy `p/1` are deterministically separated).
- `roles` array: fixed canonical order `fact`, `head`, `body`, `goal`
  (a subset in that order), never insertion order.
- `doc.malformed` array: byte-lexicographic by the offending key name.
- advisory `candidates`: the [[#REQ-406]] triggering sibling(s) first,
  then occurrences sharing the fact's resolved argument, then
  byte-lexicographic by `literal`, then by `listener` label (the final
  tie-breaker).

**Row shape.** `doc` is **always present** as an object (never elided):
for an undocumented family `description` is `null`; `documenter` is the
signer DID or `null` (built-in and undocumented families → `null`);
`redefined` is always a boolean (built-in → `false`); `malformed` is
always an array (possibly empty); `detached` is always a boolean.
Per-occurrence provenance (which rule/label an occurrence sits in) is
**not** carried on the vocab row — `roles` is the aggregate set; that
provenance lives in `dag`/`why-not` output and, for advisory candidates,
in the `listener` field.

```json
// vocab (array elements) — parameterised family: kind "predicate", arity broken out
{ "kind":"predicate", "family":"ci-green/1", "functor":"ci-green", "arity":1,
  "roles":["body"], "class":"hole", "built_in":false,
  "doc":{ "description":"CI pipeline green for task ?t",
          "kind":"evidence", "asserter":"role:ci",
          "documenter":"did:crdt:…", "redefined":false,
          "malformed":[], "detached":false } }
// legacy family: kind "legacy", no arity/functor; here undocumented
{ "kind":"legacy", "family":"deploy-thing",
  "roles":["fact"], "class":"orphan", "built_in":false,
  "doc":{ "description":null, "documenter":null, "redefined":false,
          "malformed":[], "detached":false } }
// built-in family: fixed description, no signer provenance
{ "kind":"legacy", "family":"completed",
  "roles":["body"], "class":"active", "built_in":true,
  "doc":{ "description":"task is finished (given fact; lifecycle terminal)",
          "kind":"control", "documenter":null, "redefined":false,
          "malformed":[], "detached":false } }
// assert receipt — advisory present only when REQ-406 fires
{ "v":1, "receipt":"s-…", "…":"…",
  "advisory":{ "kind":"sibling",               // or "inert-family"
               "family":"ci-green/1", "family_kind":"predicate",
               "candidates":[ { "literal":"(ci-green m1)",
                                "listener":"r-verified" } ] } }
// why-not / require — added docs object (see REQ-405 for field scope);
// each entry carries its family kind so keys never collide
{ "…":"…", "docs":{ "ci-green/1":{ "family_kind":"predicate",
                                   "description":"…", "kind":"evidence",
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

Pure core (no I/O, no clocks): `core::vocab` — a **thin composition over
spindle's [[SPEC-024 predicate model|SPEC-024]] `vocabulary` module**
(`TheorySignature::derive`, `Vocabulary::derive` for parameterised
family identity, roles, and provenance) plus elephant-only additions:
the legacy suffix resolver + built-in registry ([[#CON-402]],
[[#REQ-404]], static data), the `goal` role, per-key docs conformance +
LWW provenance ([[#ADR-403]], one pass over admitted Entries), and
advisory computation ([[#REQ-406]]) — all functions of `(ClosureResult,
admitted entries, tasks)`. `head`/`body` roles come from spindle's
`OccurrenceRole`; `fact` (a `Head` refined by rule type) and `goal`
(commitments) are elephant's; the **demanded-unprovable** set for variable
templates (the substance of `hole`/sibling — [[#REQ-401]] demand) is
*not* the forward grounding fixpoint's output but a **proven co-body
witness join** over the closure's already-grounded facts — a projection,
no fresh query, so the [[#NFR-401]]/[[#NFR-402]] budgets hold (SPEC-024's
`Vocabulary` does not touch inference, so holes stay elephant's).
Effectful shell: `cli` (new `vocab`/`define` arms, advisory rendering),
`daemon` (cached-view serving, [[#NFR-402]]). Boundary types:
`VocabView`, `FamilyDoc`, `Advisory` (wrapping spindle's
`PredicateSymbol`/`VocabularyEntry`). Dependency rule unchanged: shell →
core; enforcement per [[SPEC-001-elephant-core]] §5 (clippy
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
| TEST-401 | REQ-401 | families listed with roles composed from spindle `Vocabulary`/`OccurrenceRole` (`fact`/`head`/`body`) plus elephant `goal` (commitments; negated body counts) /class/docs; hole = unproven demanded instance, non-fact-head; orphan = unconsumed fact; detached doc flagged (occurrence-join); parameterised family `(ci-green ?t)` → family `ci-green/1` role `body`, no suffix; variable-template hole via witness join (`(review-approved m1)` proven binds `?t=m1`, `(ci-green m1)` unproven → `ci-green/1` hole); witness-less single-body template classed `active`, not `hole`; mixed corpus (`verified-m1` + `(verified m1)`) → two rows `verified` (kind `legacy`) and `verified/1` (kind `predicate`), both `built-in`; collision test: `(p x)` + a flat atom spelled `p/1` → two **distinct** rows (`kind:predicate` vs `kind:legacy`), never merged; a literal with a control-char/empty functor → `Malformed` occurrence (kind `malformed`), distinct malformed atoms not merged, not a panic/drop | per-key malformed → key marked, rest of record intact | discovery-kind or built-in fact flagged orphan → fail; documented family shown undocumented → fail; sibling-proven family masking an unproven instance's hole class → fail; parameterised template with no proven co-body witness flagged `hole` → fail; `detached` flagged on a symbol with ≥1 parameterised occurrence → fail |
| TEST-402 | REQ-402 | `(meta (predicate ci-green 1) …)` and inline `(predicate ci-green ((task symbol)) …)` recognised at view keyed on `ci-green/1`; legacy `(meta <stem> …)` label carrier still read; tombstoned doc Entry reverts to prior winner per key | legacy label naming an admitted rule → rule metadata, not family doc; unknown keys ignored, preserved for `describe`; one bogus key does not undocument the family | vocab layer mutating or dropping foreign meta keys → fail; a rule labelled `ci-green` shadowing the `ci-green/1` predicate-target doc → fail (distinct namespaces) |
| TEST-403 | REQ-403 | `define ci-green/1 --arg task:symbol --desc …` → predicate-declaration Entry, receipt, corpus+1; view shows doc keyed on `ci-green/1`; `define ci-green/1` (no `--arg`) → bare `(meta (predicate ci-green 1) …)` meta-target; `define ci-green/1` succeeds even with a task `green` declared (arity disambiguates — no 0.3.0 warning) | malformed predicate indicator, arg count ≠ arity, non-unique/bad-sort arg, >512-byte desc, control chars, bad kind, built-in collision (`define commitment-state/2`), nullary `define foo/0` (exit 3, [[#CON-402]]), leading-zero arity `define x/01`, overflowing arity `define x/4294967296` → exit 3, corpus+0 | accepted define absent from view → fail; doc keyed on wrong arity → fail |
| TEST-404 | REQ-404 | every legacy flat row resolves (`claim-v3-m1`→`claim`, `agent-alice-available`→`agent-available`, `no-deps-x`→`no-deps`); parameterised built-in matches by symbol (`(verified m1)`→`verified/1`, `(commitment-state c s)`→`commitment-state/2`); registry rows marked built-in with kinds | `define completed/1` / `define commitment-state/2` / `define verified/2` (alternate arity of a built-in functor) → exit 3; corpus predicate-meta on a built-in functor at any arity ignored | built-in family triggers advisory or orphan → fail; unlisted `deploy-v3-m1` treated as built-in → fail; `define verified/2` accepted (arity-level, not functor-level, collision) → fail |
| TEST-405 | REQ-405 | why-not/require text + `--json docs` join corpus-documented and built-in families | undocumented families → output byte-identical to pre-405 shape | doc joined to wrong family → fail; `documenter`/`redefined` leaking into docs object → fail |
| TEST-406 | REQ-406 | daemon live: inert undocumented fact → inert-family advisory; `ci-green-m2` with only `ci-green-m1` listened → sibling advisory naming `ci-green-m1` + rule label; parameterised `(ci-green m2)` while `(review-approved m1)` proven binds `?t=m1` in `(and (ci-green ?t) (review-approved ?t))` → `(ci-green m1)` demanded-unproven → sibling advisory naming `(ci-green m1)`; triggering sibling ranked first and retained under the 5-candidate cap even when ≥5 unrelated argument-sharing holes are present; `--no-advice` on the append request → NO advisory; commitment-goal assert → NO advisory (goal role) | direct mode, `--no-advice`, rule payload, negated-body-consumed family, discovery-kind, built-in → no advisory | advisory changed exit code or blocked append → fail; advisory computation failure failing the assert → fail; sibling advisory fired for a template with no proven co-body witness (no live demand) → fail; triggering sibling dropped by the 5-candidate cap → fail |
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
  name suffixes existing *flat* atoms retroactively splits their
  families ([[#ADR-402]]), detaching documentation. This vector exists
  **only on the legacy suffix path**: a
  [[Parameterised Literal|parameterised]] family resolves on its functor
  alone and is immune, so
  the structural fix is to author new vocabulary parameterised
  ([[#CON-402]] rule 1). For flat atoms that remain, mitigation:
  resolution never depends on docs (no escalation), and detached
  documentation is surfaced per family ([[#REQ-401]] `detached`), so the
  squat is visible the moment any view runs.
- **Doc shadowing via rule label — CLOSED for declared predicates.**
  Family documentation now targets the predicate symbol via spindle's
  `MetaTarget::Predicate` ([[#REQ-402]], [[#ADR-402]]), a distinct
  namespace from rule labels, so a rule *labelled* `ci-green` can no
  longer reclassify `ci-green/1`'s documentation. The vector survives
  **only** for the legacy label-string carrier (`(meta <stem> …)` on a
  flat family): there a rule label still wins, is visible (view shows the
  family undocumented while `describe` shows the colliding rule), and is
  attributable to the rule's signer. Adopting declared predicates
  eliminates it.
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
<summary>Revision history — 0.1.0 → 0.5.0</summary>

- 0.5.0 (implementing) — Phase 3 begun on stakeholder direction
  (2026-07-14, HOC): implementation plan [[IMPL-005]]
  (`plans/IMPL-005.spl`), branch `spec-005-vocab-impl`. No normative
  change.

- 0.5.0 (draft) — **reviewer round applied (8 findings, 3 blocking).**
  (#1, High) [[#CON-402]] `family` now returns a **discriminated
  `Family`** — `Predicate(PredicateSymbol) | Legacy(String) |
  Malformed(String)` — not a bare `String`, closing the collision where a
  parameterised `(p x)` and a flat atom literally spelled `p/1` (legal
  SPL: `/`, `<`, `>` are atom chars) both keyed `"p/1"`, and where the
  `<malformed>` sentinel was itself a legal flat atom; the discriminant
  rides the wire as [[#CON-403]] `kind`. (#2, High) [[#REQ-401]]
  *Grounding and demand* rewritten: a **supported fragment** (flat
  literals; templates all of whose variables are bound by the proven
  co-body witness join, generalised to multiple variables) with a pinned
  definition of **proven fact** (accepted weighted-conclusion under the
  local trust policy), and an explicit **out-of-fragment** list (unbound/
  partial templates, ground goals with no proven binder, arithmetic and
  temporal args, malformed occurrences) that contributes no demand →
  `active`, a bounded named deferral. (#3, High) the daemon advisory
  contract is now expressible: [[SPEC-002-elephant-p2p#CON-101]] append
  request/response amended additively with an `advice` control and an
  `advisory` object (SPEC-002 → 0.1.3), and [[#NFR-402]] defines the
  reference view's initialization/refresh. (#4) nullary `functor/0`:
  [[#CON-402]] rule 1 requires arity ≥ 1, arity-0 stays legacy, `define
  foo/0` is refused ([[#REQ-403]]), a raw `foo/0` predicate-meta shows
  `detached`. (#5) [[#REQ-406]] guarantees the triggering same-family
  sibling is ranked first and never dropped by the 5-candidate cap. (#6)
  [[#CON-401]] ABNF made order-free with `description` optional at grammar
  level, and `arity` constrained (no leading zeros, ≤ 2^32−1). (#7)
  [[#CON-403]] completed: `kind` discriminant, `roles`/`malformed`
  orderings, candidate `listener` tie-breaker, always-present `doc` object
  (undocumented → `description:null`), built-in `documenter:null`/
  `redefined:false`, legacy-arity representation, occurrence-provenance
  scope; [[#REQ-404]] now ships the **fixed built-in description texts**
  and moves `failed` to its own `control` row (was contradictorily in the
  discovery row). (#8) [[#CON-402]] defines the `tasks` set precisely — the
  closure's provable-`task-X` conclusions ([[SPEC-003-elephant-tasks]] §1
  `(given task-X)`), not rule heads or raw givens — and the stale
  `src/tasks.rs` reference (task layer retired) is removed. Normative
  surface: family type, demand definition, `define`/`CON-401` arity rules,
  `CON-403` shape, and the SPEC-002 append contract all changed. Still
  draft; awaiting a fresh-context adversarial pass on this round and
  stakeholder validation.

- 0.4.0 (draft) — **compose SPEC-024** (elephant's spindle dependency
  moved to `main`, which merged SPEC-024's predicate-model / vocabulary
  layer via PR #34; elephant `cargo check` clean against it). The 0.3.0
  phased deferrals become adopted decisions: a [[Predicate Family]] is
  now spindle's [[Predicate Symbol|`PredicateSymbol`]] `(functor, arity)`
  composed from `TheorySignature`/`Vocabulary::derive` ([[#CON-402]] rule
  1, [[#ADR-402]] retitled), with `fact`/`head`/`body` roles read from
  spindle's `OccurrenceRole` and elephant adding only the `goal` role,
  the legacy suffix resolver, and the `hole`/demand witness join (SPEC-024
  does not touch inference, so holes stay elephant's — [[#REQ-401]], §5).
  Documentation now rides the **`MetaTarget::Predicate`** carrier —
  `(meta (predicate ci-green 1) …)` or inline on a `(predicate …)`
  declaration ([[#REQ-402]], §1.3) — which **closes the rule-label
  shadow vector outright** for declared predicates (§8) and dissolves the
  0.3.0 `define` functor-vs-flat-stem ambiguity: arity disambiguates, so
  [[#REQ-403]] `define <functor>/<arity>` fully recognises again (no
  warning), emitting a declaration or meta-target. [[#CON-401]] gains the
  predicate-indicator / declaration / sort grammar; [[#REQ-404]] built-ins
  key by symbol (`verified/1`, `commitment-state/2`); [[#CON-403]] `family`
  becomes `functor/arity` with `arity` broken out; [[#ADR-403]] pins the
  per-(`MetaTarget`, key) LWW. Legacy flat vocabulary keeps its suffix
  resolution and label carrier throughout. Residual `Open`: goal-only
  single-body demand still under-reported; SPEC-024 is itself `draft`
  (API-churn risk, bounded by its compat wrappers). Normative surface:
  family identity, doc carrier, and `define` signature all changed.
  **Adversarial pass on the composition applied** (fresh-context, spindle
  source as ground truth; 8 findings): family identity is a
  *distinct-key* pair — `(verified ?t)` → `verified/1`, `verified-<task>`
  → `verified`, both reserved built-ins, a mixed corpus showing both rows
  (accepted projection, not a contradiction); `fact`/`goal` are
  elephant-added roles (spindle's `OccurrenceRole` is `Head`/`Body` only,
  so `fact` refines a `Head` by rule type); built-in reservation pinned
  as **functor-level at every arity** ([[#REQ-404]], [[#CON-401]]); the
  "composed from `Vocabulary::derive`" claim scoped to rule/fact
  occurrences (legacy families re-group spindle's arity-0 symbols,
  goal-only families are elephant-synthesized); [[#CON-402]] given an
  explicit `Err` branch for a malformed functor (`predicate_symbol()` is
  fallible), preserving totality; the "§8 vector closed" claims qualified
  "for declared predicates"; TEST-401/404 gained the double-row,
  alternate-arity, and malformed-functor cases. Still draft; awaiting
  stakeholder validation.

- 0.3.0 (draft) — **predicate-support revision** (spindle now carries
  first-class [[Parameterised Literal|parameterised literals]] + first-order
  variables; grounding discriminates `(p a)` from `(p b)` at reasoning
  time). Phased scope: target the parameterised-literal capability
  elephant *compiles against today*, and record composing spindle's
  richer SPEC-024 predicate-model / `vocabulary` layer (on `origin/main`,
  not yet in elephant's dependency) as the next deferral with a named
  trigger — the spec is not written against an un-merged branch. Changes:
  [[#CON-402]] rule 1 (family = functor) reframed as the primary,
  task-independent path, RECOMMENDED for new vocabulary; the suffix rules
  (2–3) retained only for the frozen/legacy flat vocabulary. §1.3 coining
  convention now shows a single variable-quantified evidence rule
  (`(normally r (and (ci-green ?t) (review-approved ?t)) (verified ?t))`)
  in place of one hyphen-packed rule per task. [[#ADR-402]] retitled and
  its Deferred section rewritten: parameterised literals are adopted (no
  longer deferred), the new deferral is the SPEC-024 `vocabulary`-module
  composition (Simplicity-Ladder rung-4 simplification) with
  `MetaTarget::Predicate` as the cleaner documentation carrier. [[#REQ-401]]
  gains an explicit **grounding** clause (roles read at family level from
  templates; hole/orphan/candidate enumeration over the grounded theory),
  and `detached` noted as unreachable for parameterised families.
  [[#REQ-402]] notes the family atom is the functor and the carrier stays
  label-string-keyed pending SPEC-024. [[#REQ-404]] notes built-ins match
  their parameterised form via [[#CON-402]] rule 1. [[#REQ-405]] keys the
  docs join on the exact ground instances spindle's `why-not`/`require`
  now return. [[#REQ-406]] sibling advisory re-expressed over grounded
  demands, covering `(ci-green m2)` vs `(ci-green m1)` alongside the legacy
  flat spelling. §5 Purity Map and §8 (squatting confined to flat atoms +
  parameterised-as-mitigation; rule-label shadow closed structurally by the
  deferred `MetaTarget::Predicate`) updated; TEST-401/404/406 gain
  parameterised cases. **Adversarial round 2 (fresh-context) applied**:
  (1) the demanded-unprovable instance set for variable templates —
  the substance of `hole`/sibling — was wrongly attributed to the
  forward grounding fixpoint, which materialises only *provable*
  derivations; redefined as a **proven co-body witness join** over the
  closure's already-grounded facts (a projection, no fresh abduction, so
  [[#NFR-401]]/[[#NFR-402]] hold), with the witness-less single-body /
  goal-only case explicitly under-reported and deferred to the SPEC-024
  composition ([[#REQ-401]], [[#REQ-406]], §5). (2) `detached` redefined
  as occurrence-join (a functor with ≥1 parameterised occurrence is
  never detached) rather than bare-label resolution, and the false
  "parameterised families can't detach" immunity dropped ([[#REQ-401]]).
  (3) `define`'s self-resolution refusal downgraded to a **warning**: a
  bare name has no arity, so refusing would gatekeep the RECOMMENDED
  functor path; robust functor-vs-instance recognition deferred to
  `MetaTarget::Predicate` ([[#REQ-403]], [[#CON-401]]). (4) the sibling
  advisory and TEST-401/403/406 made realizable with an explicit binding
  witness (they previously pinned a circular/vacuous scenario). Still
  draft; normative surface unchanged except CON-402's primary/legacy
  framing, the define warn-vs-refuse change, and the witness-join
  demand definition. Awaiting stakeholder validation.

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
