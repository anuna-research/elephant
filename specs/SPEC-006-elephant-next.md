---
id: SPEC-006
title: elephant next — theory-derived task discovery
version: 0.3.0
status: implemented
date: 2026-08-09
audience: agent, human reviewer
---

# SPEC-006 — elephant next

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL in this document are to be interpreted as
described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear in
all capitals.

## Orientation

Intent: Let an enrolled [[users/agent/user|autonomous agent]] ask one
[[Logical Theory]] what well-described work it may take next. The answer must
be derived from Elephant's signed corpus, not recreated by a process harness
or a local plan-file parser.

Metaphor: a minute-book with a dispatch tab. The tab points to an agreed next
piece of work and the minutes that justify it; it does not chair the meeting
or perform the work.

Structure:

```
agent ──elephant next──▶ closure + commitment ledger ──▶ candidate list
  │                              │                              │
  │                              ▼                              ▼
  │                    predicate args + rule meta         promise command
  │                              │                              │
  └── explain (ready X) ◀────────┴── describe r-ready ──────────┘
```

Decisions: [[SPEC-006-elephant-next#ADR-601]] native read-only discovery ·
[[SPEC-006-elephant-next#ADR-602]] documentation is an eligibility gate ·
[[SPEC-006-elephant-next#ADR-603]] suggested commands are tokens, not shell
text · [[SPEC-006-elephant-next#ADR-604]] tasks are identified by predicate
argument, not by name suffix.

Load-bearing: [[SPEC-006-elephant-next#REQ-601]] theory-derived candidates ·
[[SPEC-006-elephant-next#REQ-602]] no corpus mutation ·
[[SPEC-006-elephant-next#REQ-604]] complete documentation ·
[[SPEC-006-elephant-next#REQ-605]] proof and provenance.

Controls: [[SPEC-006-elephant-next#REQ-602]] `next` SHALL NOT append or
retract any [[Speech Act]] · [[SPEC-006-elephant-next#REQ-603]] it SHALL NOT
accept a `.spl` plan path, assign work, or schedule workers ·
[[SPEC-006-elephant-next#REQ-604]] incomplete task documentation SHALL be
withheld, never presented as actionable.

Open: [[SPEC-006-elephant-next#OPEN-601]] prose lives in an interned
`Term::Symbol`, so a task's documentation is a signed fact rather than a
`meta` property (owner: HOC).

Detail: [[users/agent/user]] · [[users/agent/happy-paths#HP-A2]] ·
[[SPEC-001-elephant-core#REQ-010]] ·
[[SPEC-003-elephant-tasks#ADR-206]] ·
[[SPEC-005-elephant-vocabulary#ADR-402]] ·
[[SPEC-005-elephant-vocabulary#REQ-402]] ·
[[SPEC-006-elephant-next#Requirements]] ·
[[SPEC-006-elephant-next#Contracts]] · [[SPEC-006-elephant-next#Tests]].

## Amendment Channels

Amendable by: HOC, the Elephant specification steward.

Through: a merged revision of this specification, or a linked `ADR-###` that
is merged before the implementation changes.

Not amendable by: agent prompts, corpus contents, generated task descriptions,
or a request returned by `elephant next`.

Hard stops: [[SPEC-006-elephant-next#REQ-602]],
[[SPEC-006-elephant-next#REQ-603]], and
[[SPEC-006-elephant-next#REQ-604]] require a new specification version to
change.

## Context

[[hence]] made `task next plan.spl` useful because one
read-only command projected ready work from a plan. Elephant intentionally
retired that complete task layer in [[SPEC-003-elephant-tasks#ADR-206]]:
there is no local plan file, assignment database, claim verb, scheduler, or
supervisor to recover. A theory is instead an append-only corpus of signed
[[Speech Act]]s whose [[SPEC-001-elephant-core#CON-003|closure]] determines
what holds.

That retirement left a genuine usability gap. The agent profile's daily
workflow already names `elephant next --json`, but current agents reconstruct
it manually with `status`, `commitments`, `explain`, and `describe`. They can
therefore miss a ready task, treat an outstanding [[Commitment]] as invisible,
or ignore the documentation that explains the work and its acceptance
criterion.

This specification restores only the query. `elephant next -t THEORY` is a
native projection over existing closure, commitment, and documentation views.
It does not restore hence's `.spl` carrier or its task lifecycle writers.

## User Profile and Happy Path

The autonomous agent in [[users/agent/user]] needs a short, machine-readable
answer to “what work is available now?” before it chooses whether to make a
promise.
It is already an enrolled theory member; this command neither creates nor
joins a theory.

The amended form of [[users/agent/happy-paths#HP-A2]] is:

1. The agent runs `elephant next -t release-v3 --json`.
2. It reads one candidate's description, acceptance criterion, readiness
   literal, and source-bearing readiness rule.
3. It runs the returned `explain` and `describe` token arrays before acting.
4. It independently decides whether it can meet the criterion.
5. It executes the returned `promise` token array, does the work, and records
   evidence with an existing producer command.

If there is no candidate, the agent is idle rather than failed. If a task is
ready but its documentation is incomplete, the agent receives a repairable
withheld diagnostic and does not promise it.

## Task Convention

This specification defines the task convention consumed by `next`. It is an
opt-in projection of legal [[SPL]], not a new theory grammar or scheduler.

A task is identified by a **symbol term in a predicate argument**, following
the quantified pattern [[SPEC-005-elephant-vocabulary#ADR-402]] already
recommends. It is not identified by a hyphen suffix on an atom name. For a
task identifier `X`, a candidate author supplies these forms:

```lisp
; task, ready and completed are RESERVED built-in control vocabulary
; (SPEC-005 REQ-404) at every arity. They are neither declared nor
; declarable here — `define task/1` is refused at the wire. The
; convention consumes the built-in families rather than coining rivals.

; Only the coined documentation predicates are declared.
(predicate task-description ((id symbol) (text symbol))
           (description "prose statement of the work for ?x") (kind state))
(predicate task-acceptance ((id symbol) (text symbol))
           (description "governing acceptance criterion for ?x") (kind state))

; Per task — the identifier is an argument, never part of a functor.
(given (task models))
(given (task-description models "Design the data model"))
(given (task-acceptance models "TEST-601 fixture acceptance passes"))
(given (prerequisite-met models))

; One quantified readiness rule serves every task.
(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))
(meta r-ready (source "SPEC-006-elephant-next#TEST-601"))
```

`kind` values are constrained by [[SPEC-005-elephant-vocabulary#CON-401]] to
`evidence`, `state`, or `discovery`; `control` is built-in only. The
documentation predicates take `state`, being stable properties of a task
rather than evidence of a condition or a discovery.

Four consequences follow, and they are the substance of this convention.

**The task vocabulary is already reserved.** Because `task`, `ready`, and
`completed` are built-in control families, a theory cannot coin a rival
spelling of them, and `next` needs no declaration to recognise them. The
built-in registry describes `task` as a "declared unit of work (legacy hence
lifecycle)" — this specification gives that reserved vocabulary a live
consumer again, at predicate arity rather than as a flat atom.

**One rule, not one rule per task.** `r-ready` is a single quantified rule
whose instances spindle grounds per binding of `?x`. A theory with forty tasks
has one readiness rule, not forty. A theory MAY declare several readiness
rules (`r-ready`, `r-ready-hotfix`); each candidate is attributed to whichever
rule actually derived its `(ready X)` conclusion.

**Documentation is a fact, not a `meta` property.** Spindle's `MetaTarget` is
either a label or a predicate symbol; there is no per-instance target, so
`(meta (task models) …)` is not expressible. Per-task prose therefore rides in
`task-description/2` and `task-acceptance/2` as ordinary signed facts. Their
second argument is an interned `Term::Symbol` carrying the quoted text. This
is recorded and justified in [[SPEC-006-elephant-next#ADR-604]] and its
residual cost in [[SPEC-006-elephant-next#OPEN-601]].

**Rule provenance still uses `meta`.** A rule label is a label, so
`(meta r-ready (source …))` is the existing [[SPEC-005-elephant-vocabulary#REQ-402]]
carrier, unchanged. Only per-task prose moved.

`task-description` and `task-acceptance` are deliberately not spelled
`description` and `acceptance`: `description` is already a `meta` **key** in
[[SPEC-005-elephant-vocabulary#CON-401]], and reusing the word as a predicate
functor would make review harder for no gain.

Task authors put holds, priorities, review requirements, and dependencies in
the body of the readiness rule. `next` projects the resulting closure without
another decision system.

## Requirements

### REQ-601: Theory-Derived Candidate Set

`elephant next` SHALL render a task identifier `X` as a candidate only when
all of these conditions hold:

- The selected theory positively derives `(task X)` and `(ready X)`.
- The theory does not positively derive `(completed X)`.
- No outstanding [[Commitment]] has a goal structurally equal to `(completed X)`.

Goal equality SHALL reuse the existing commitment-reflection comparison from
[[SPEC-001-elephant-core#CON-003]]: both sides are parsed through the one SPL
recogniser and compared in canonical `to_spl()` form. It is never a comparison
of raw user text or of human display output.

For the task identifiers this projection recognises — arg 0 is a
`Term::Symbol` per [[SPEC-006-elephant-next#CON-601]] — canonical-form equality
and spindle `Literal` equality coincide, so no separate comparison path is
introduced. They can diverge only for numeric argument terms, which
[[SPEC-006-elephant-next#CON-601]] already excludes from the convention.

Only an **unqualified** literal counts. A modal or temporally-bounded
conclusion — `(must (ready X))`, `(during (ready X) …)` — SHALL NOT satisfy
the readiness condition. Spindle carries `Mode` and `Temporal` alongside the
predicate name, so such a literal reports name `ready` at arity 1 while the
theory does not derive `(ready X)`; admitting it would emit a `ready_literal`
the reasoner cannot prove and a [[SPEC-006-elephant-next#REQ-605]] receipt
that contradicts its own candidate.

Every literal the command renders or compares SHALL be produced through
spindle's `Literal` and its canonical `to_spl()`. The command SHALL NOT build
one by interpolating a task identifier into a string. A task identifier is any
`Term::Symbol`, including a quoted symbol containing spaces, parentheses, or
quotes; `"(completed " + X + ")"` renders `my task` as the two-argument
literal `(completed my task)`, silently changing the predicate symbol so an
outstanding commitment stops matching and the emitted operand names a literal
the theory never derives.

The command does not infer readiness from file position, source order, agent
identity, an assignment literal, or an unproved rule head.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-601]]
- [[SPEC-006-elephant-next#TEST-602]]
- [[SPEC-006-elephant-next#OBS-601]]

### REQ-602: Read-Only Query Boundary

`elephant next` SHALL NOT append, retract, concede, promise, request, sign,
sync, or otherwise mutate the selected theory, its membership, or its local
identity state.

The query uses the existing local replica and its already-admitted corpus. It
does not cause a network operation merely because an agent inspected its next
action.

Trace:

- [[SPEC-006-elephant-next#CON-602]]
- [[SPEC-006-elephant-next#TEST-606]]
- [[SPEC-006-elephant-next#OBS-601]]

### REQ-603: Native Query Scope

`elephant next` SHALL remain one flat read command, selected with the existing
global `-t` theory selector. It SHALL NOT accept a plan-file path, recreate
the `task` command group, assign an agent, claim a task, start a worker, or
run a scheduler.

This preserves [[SPEC-003-elephant-tasks#ADR-205]]'s one-spelling rule and
[[SPEC-003-elephant-tasks#ADR-206]]'s removal of the hence task layer. The
command answers what the shared theory says; ownership starts only when an
agent chooses an existing `promise` producer.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-607]]

### REQ-604: Complete Documentation Is a Selection Gate

`elephant next` SHALL withhold a ready, non-terminal task when any of these
values is absent:

- a positively derived `(task-description X …)` fact;
- a positively derived `(task-acceptance X …)` fact;
- the label of the rule that derived `(ready X)`; or
- that rule label's resolved `source` metadata.

The output SHALL identify the task and a stable withheld reason. A bare
identifier does not make a task actionable. The next agent needs the work,
completion criterion, and readiness source before making a promise.

Where a task has more than one positively derived `task-description/2` or
`task-acceptance/2` fact, the projection SHALL select the bytewise-least text
argument and SHALL NOT treat the multiplicity as an error. Documentation is
inert, so a duplicate is an authoring wart rather than a contradiction, and a
deterministic choice keeps [[SPEC-006-elephant-next#NFR-601]] satisfiable.

A rule's resolved `source` SHALL NOT satisfy this gate when its value is an
envelope-derived agent atom — the `agent:` prefix reserved by
[[SPEC-001-elephant-core#ADR-012]]. Every claims-wrapped assert has that
provenance attached automatically, so without this exclusion the readiness
source is never absent, `missing-ready-source` is unreachable through the
producer path, and `next` reports *who* asserted the rule as though it were
the citation for *why* the work is ready. Likewise a list-valued `source` is
well-formed metadata but not a single citation, and SHALL be treated as
absent rather than resolved to an arbitrary element.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-603]]
- [[SPEC-006-elephant-next#TEST-604]]
- [[SPEC-006-elephant-next#TEST-610]]
- [[SPEC-006-elephant-next#TEST-612]]
- [[SPEC-006-elephant-next#OBS-601]]

### REQ-605: Proof and Provenance Receipt

`elephant next --json` SHALL include the resolved description, acceptance,
ready literal, readiness-rule label, and source for every candidate. It SHALL
also include token arrays that invoke the existing `promise`, `explain`, and
`describe --json` commands for that exact task and theory.

The emitted rule label SHALL be the **template** label. Spindle names a
grounded instance of a quantified rule `{template}_{n}`; only the template
carries metadata, so emitting `r-ready_3` would produce a `describe`
invocation that resolves nothing.

Template recovery SHALL be existence-driven, not textual: the conclusion's
rule label is looked up in the theory as-is first, and a trailing `_<digits>`
run is stripped only until a prefix names a rule that exists. This mirrors
spindle's own `explanation::resolve_rule`, so `next` and `explain` attribute
one conclusion to one rule. Blind stripping SHALL NOT be used — it would
rewrite a rule genuinely named `phase_2` to `phase` and attribute that task's
readiness to a different rule, or to none. When no prefix names an existing
rule, the task is withheld as `missing-ready-rule` rather than reported
against a guessed label.

Where several rules derive one `(ready X)`, the projection SHALL report the
bytewise-least resolved template, so attribution never depends on the order
the reasoner happens to emit conclusions in.

[[SPEC-001-elephant-core#REQ-011|`explain`]] proves the readiness literal and
its firing rule. It does not render rule annotations. The returned
`describe --json` token array reads the template's `meta.source`. This
separation preserves each existing query's meaning and makes provenance
operational.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-601]]
- [[SPEC-006-elephant-next#TEST-605]]
- [[SPEC-006-elephant-next#TEST-611]]
- [[SPEC-006-elephant-next#TEST-613]]
- [[SPEC-006-elephant-next#OBS-601]]

### REQ-606: Idle Is a Successful Observation

`elephant next --json` SHALL exit zero and return an empty `next` array when
no task meets [[SPEC-006-elephant-next#REQ-601]] and
[[SPEC-006-elephant-next#REQ-604]]. A valid empty selection is not an error,
because a short-lived agent needs to distinguish idle from unavailable theory,
malformed corpus, or failed closure.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-608]]
- [[SPEC-006-elephant-next#OBS-601]]

### NFR-601: Deterministic, Single-Closure Projection

For identical admitted corpus, trust policy, and evaluation time, `elephant
next --json` SHALL produce byte-identical arrays in bytewise task-identifier
order. It SHALL reuse the closure evaluation and commitment reflection from
[[SPEC-001-elephant-core#CON-003]]. It SHALL NOT run a second reasoner pass or
an abduction query.

The readiness-rule label is available on the conclusion recorded by that one
closure evaluation. Recovering it SHALL be a lookup over the computed
conclusions, not a call that re-derives the literal.

Trace:

- [[SPEC-006-elephant-next#CON-602]]
- [[SPEC-006-elephant-next#TEST-605]]
- [[SPEC-006-elephant-next#TEST-609]]
- [[SPEC-006-elephant-next#OBS-601]]

## Contracts

### CON-601: `next` CLI and JSON Contract

Interface: `elephant next -t THEORY [--json]`.

`THEORY` is recognised by the existing global theory-selector grammar in
[[SPEC-001-elephant-core#REQ-003]]. `next` introduces no positional
arguments and no option that carries a task identifier, agent name, plan path,
or shell fragment. The task identifiers it projects are parsed [[SPL]] terms
from the admitted closure, not newly accepted user input.

The convention is recognised over **predicate symbols and argument terms**:

| Role | Predicate symbol | Recognised shape |
| --- | --- | --- |
| task exists | `task/1` | arg 0 is `Term::Symbol` |
| readiness | `ready/1` | arg 0 is `Term::Symbol` |
| terminal | `completed/1` | arg 0 is `Term::Symbol` |
| work prose | `task-description/2` | arg 0 identifier, arg 1 text |
| criterion prose | `task-acceptance/2` | arg 0 identifier, arg 1 text |

The task identifier `X` is the arg-0 `Term::Symbol` itself. There is no
identifier grammar to enforce and no suffix to strip, because the identifier
was never packed into a functor. A closure conclusion whose predicate symbol
is outside this table, or whose arg 0 is a non-symbol term, is ignored by this
projection and remains visible through `status` and `trace`.

The implementation compares interned predicate symbols and argument terms. It
MUST NOT recover a task by substring search over raw SPL or output text, and
MUST NOT match `task/1` against a differently-arity `task/2`.

Successful JSON output has this stable shape. `theory` is the resolved theory
id; aliases are not emitted as authorities. `next` and `withheld` are sorted
by bytewise `task`. Human output is an informative projection of this object,
not a second contract.

```json
{
  "v": 1,
  "theory": "<64-lowercase-hex-id>",
  "next": [
    {
      "task": "models",
      "description": "Design the data model",
      "acceptance": "TEST-601 fixture acceptance passes",
      "ready_literal": "(ready models)",
      "promise_goal": "(completed models)",
      "ready_rule": "r-ready",
      "source": "SPEC-006-elephant-next#TEST-601",
      "commands": {
        "promise": ["elephant", "promise", "(completed models)", "-t", "<theory-id>"],
        "explain": ["elephant", "explain", "(ready models)", "-t", "<theory-id>"],
        "describe": ["elephant", "describe", "r-ready", "-t", "<theory-id>", "--json"]
      }
    }
  ],
  "withheld": [
    { "task": "api", "reason": "missing-ready-source" }
  ]
}
```

`ready_literal` and the `promise` / `explain` operands SHALL be rendered by
spindle's canonical `Literal::to_spl()`, which is the parenthesised form
`(ready models)`.

They SHALL NOT be rendered by `closure::literal_display`. That helper strips
the outer parentheses from a positive literal for human output, yielding
`ready models`; it is bound to the status view and the closure fingerprint,
and is not a machine operand. Both forms happen to re-parse through
`parse_literal` today, so the distinction is not self-correcting — it has to
be specified. `to_spl()` is also the form the commitment ledger already
reports as `commitments[].goal`, so a `promise` token array and the ledger
entry it produces render identically.

The `commands` members are JSON token arrays, not commands passed to a shell.
The consumer executes an array only after its own approval and argument
handling. The `promise` array is an invitation to take work; rendering it does
not fulfil or create a [[Commitment]].

`withheld.reason` is one of `completed`, `outstanding-commitment`,
`missing-task-description`, `missing-task-acceptance`, `missing-ready-rule`,
or `missing-ready-source`. When several reasons apply, the command selects the
first in that listed order, making the diagnostic deterministic. A task that
is not positively ready is omitted from both arrays.

Pre-conditions:

- `THEORY` resolves to a locally-held theory through the existing selector.
- The local identity and corpus are readable under the existing core contract.

Post-conditions:

- The response is computed from the admitted closure, commitment states, and
  conclusions of that one theory.
- `next` contains precisely the tasks eligible under
  [[SPEC-006-elephant-next#REQ-601]] and
  [[SPEC-006-elephant-next#REQ-604]].
- No candidate or withheld item is derived from an untrusted raw plan file.

Error model:

- An unknown theory, unavailable local store, or closure failure preserves the
  existing core error code and emits its existing JSON error object on stderr.
- A theory with zero candidates is a successful response per
  [[SPEC-006-elephant-next#REQ-606]].

Implements:

- [[SPEC-006-elephant-next#REQ-601]]
- [[SPEC-006-elephant-next#REQ-603]]
- [[SPEC-006-elephant-next#REQ-604]]
- [[SPEC-006-elephant-next#REQ-605]]
- [[SPEC-006-elephant-next#REQ-606]]

Verified by:

- [[SPEC-006-elephant-next#TEST-601]]
- [[SPEC-006-elephant-next#TEST-602]]
- [[SPEC-006-elephant-next#TEST-603]]
- [[SPEC-006-elephant-next#TEST-604]]
- [[SPEC-006-elephant-next#TEST-605]]
- [[SPEC-006-elephant-next#TEST-607]]
- [[SPEC-006-elephant-next#TEST-608]]
- [[SPEC-006-elephant-next#TEST-610]]
- [[SPEC-006-elephant-next#TEST-611]]
- [[SPEC-006-elephant-next#TEST-612]]

### CON-602: Read-Only Projection Boundary

Interface: the implementation behind `elephant next`.

The pure core receives an already-admitted corpus, trust policy, evaluation
time, closure, and commitment-state table. It returns the JSON-domain
selection object described by [[SPEC-006-elephant-next#CON-601]]. The
effectful shell loads those inputs and writes stdout or tracing output.

The core MUST NOT hold a signer, store handle, daemon client, network handle,
clock, subprocess handle, or filesystem path. The shell MUST NOT append an
Entry, trigger sync, or invoke a producer while serving `next`. This is the
[[SPEC-001-elephant-core#CON-003|closure]] purity boundary applied to a new
projection rather than a new lifecycle mechanism.

Implements:

- [[SPEC-006-elephant-next#REQ-602]]
- [[SPEC-006-elephant-next#NFR-601]]

Verified by:

- [[SPEC-006-elephant-next#TEST-606]]
- [[SPEC-006-elephant-next#TEST-609]]

## Architecture Decisions

### ADR-601: Native Read-Only Discovery, Not hence Parity

Elephant will add the flat `next` query because task selection is an
interpretation of Elephant's closure, commitment states, and documentation.
Putting that interpretation in Circus or another process harness duplicates
the corpus projection or forces a lossy export boundary.

This is deliberately not the historical `hence task next plan.spl` port. The
command has no `.spl` file input, assignment filter, fallback assignment set,
claim command, or task-group alias. `next` is a new native query compatible
with [[SPEC-003-elephant-tasks#ADR-206]], not a reversal of it.

Simplicity Ladder placement: rung 4. The existing closure pipeline,
commitment reflection, global theory selector, and JSON renderer already
provide all but the small pure selection projection. A new scheduler, plan
parser, task database, or process harness exceeds the first sufficient rung.

### ADR-602: Documentation Is Required Before Work Is Suggested

The command will withhold incomplete tasks instead of returning bare
identifiers. Task documentation drives no rule, so this choice preserves the
theory's semantics while making the corpus actionable. The agent needs the
description and acceptance text before promising. The reviewer needs the
source-bearing readiness rule before assessing availability.

Alternative: display all ready tasks and show missing documentation as
warnings. Rejected: an agent loop will normally take the first available item,
which makes the advice too easy to ignore. Withholding is reversible: the
theory author asserts the missing fact; no task state needs rewriting.

### ADR-604: Tasks Are Identified by Predicate Argument

`next` will recognise `(task ?x)`, `(ready ?x)`, and `(completed ?x)` and take
the task identifier from the argument term. It will not recognise the flat
`task-X` / `ready-X` / `completed-X` / `r-ready-X` atom family that version
0.1.0 of this specification defined.

The flat form made the identifier part of the functor. That forced four
consequences this project should not accept: an ABNF identifier grammar
invented solely so `next` could split names; a readiness rule duplicated per
task; recognition by string prefix, which
[[SPEC-005-elephant-vocabulary#ADR-402]] classes as legacy suffix resolution
retained only for frozen pre-predicate corpora; and no way to quantify over
tasks in any other rule. [[SPEC-005-elephant-vocabulary#ADR-402]] already
RECOMMENDS the quantified predicate form, so version 0.1.0 was in tension with
its own sibling specification.

Under predicates, one `r-ready` rule serves every task, the identifier is an
interned symbol compared by term equality, and any other rule in the theory
can bind `?x` for its own purposes. The `describe` receipt targets one shared
rule label rather than N per-task labels.

Cost, accepted: spindle's `MetaTarget` admits only a label or a predicate
symbol, so per-task prose cannot ride on `meta` and becomes the
`task-description/2` and `task-acceptance/2` facts of the task convention.
Documentation therefore enters the closure and the closure fingerprint.
See [[SPEC-006-elephant-next#OPEN-601]].

Alternative: extend spindle with a per-instance `MetaTarget` so
`(meta (task models) …)` parses. Rejected for this version: it is a
cross-repository language change to SPEC-024's predicate model in service of
one consumer, and the fact carrier is expressible in the language today. It
remains the right fix if documentation-in-closure proves costly.

Alternative: keep flat atoms and defer predicates to a later specification, as
version 0.1.0 proposed. Rejected: it ships a convention that the vocabulary
specification already calls legacy, and every corpus authored against it would
need migrating.

### ADR-603: Suggested Commands Are Token Arrays

The query will return command argument vectors instead of shell strings. This
gives a headless agent an exact next producer/query invocation without making
its JSON a code-execution channel. It also avoids shell quoting differences
and retains the user's independent approval before a `promise` changes shared
state.

This matters more under [[SPEC-006-elephant-next#ADR-604]] than it did under
flat atoms: a predicate-form operand such as `(ready models)` contains
parentheses and spaces, so a prose command line would have to be quoted
correctly by every client. A token array carries the operand as one element
and removes the question.

Alternative: return only prose commands. Rejected: each client reconstructs
quoting and argument ordering, creating the vocabulary drift this feature
removes.

The same reasoning binds the human projection: it SHALL NOT print a
copy-pasteable shell command embedding corpus text. A task identifier,
description, or acceptance string is authored by a theory member, and a
single quote inside one would break out of surrounding shell quoting and
execute whatever followed. Human output therefore names the goal without
wrapping it in shell syntax, and replaces C0/C1 control characters so corpus
content cannot repaint the terminal. `--json` carries the token array for
programmatic use.

## Open Questions

### OPEN-601: Documentation Facts Enter the Closure

Because per-task prose is a fact rather than a `meta` property
([[SPEC-006-elephant-next#ADR-604]]), `(task-description models "…")` is an
admitted conclusion. It therefore appears in `status` output and contributes
to the closure fingerprint, where the 0.1.0 `meta` carrier would not have.

This is observed, not predicted. On elephant 0.1.6 a four-fact task fixture
gives:

```
 +D  prerequisite-met models
 +d  ready models
 +D  task models
 +D  task-acceptance models "TEST-601 fixture acceptance passes"
 +D  task-description models "Design the data model"
```

Two of the five default `status` rows are prose. At forty tasks that is eighty
documentation rows in the operator's primary view.

Nothing reasons over these predicates, so there is no inferential effect. The
open question is ergonomic and diagnostic: whether `status` should learn to
fold documentation predicates out of its default view behind a flag, and
whether
a corpus that re-states a description should be advised against by
[[SPEC-005-elephant-vocabulary#REQ-406]]. Resolve before implementation
starts (owner: HOC).

## Tests

### TEST-601: Render a Fully Described Ready Task

Validates: [[SPEC-006-elephant-next#REQ-601]],
[[SPEC-006-elephant-next#REQ-605]], and
[[SPEC-006-elephant-next#CON-601]].

Given a fixture theory asserting `(task models)`, `(task-description models …)`,
`(task-acceptance models …)`, a quantified `r-ready` rule with `meta.source`,
and provable `(ready models)`. When `elephant next -t fixture --json` runs, it
returns one `models` candidate with the prose, source, and token arrays from
[[SPEC-006-elephant-next#CON-601]].

### TEST-602: Exclude Completed and Promised Tasks

Validates: [[SPEC-006-elephant-next#REQ-601]] and
[[SPEC-006-elephant-next#CON-601]].

Given two valid ready tasks, one has provable `(completed api)`. Another has
an outstanding commitment whose goal is structurally equal to `(completed ui)`
but spelled non-canonically, so a raw-text comparison would miss it.
When `elephant next --json` runs, neither appears in `next`. Each appears in
`withheld` with its stable reason.

### TEST-603: Withhold a Task Missing Work Documentation

Validates: [[SPEC-006-elephant-next#REQ-604]] and
[[SPEC-006-elephant-next#CON-601]].

Given provable `(task cache)` and `(ready cache)`, the theory omits either the
`task-description/2` or the `task-acceptance/2` fact. When `elephant next
--json` runs, `cache` does not appear in `next`. It appears once in `withheld`
with the matching reason.

### TEST-604: Withhold a Task Missing Readiness Provenance

Validates: [[SPEC-006-elephant-next#REQ-604]] and
[[SPEC-006-elephant-next#CON-601]].

Given a ready task with complete documentation, the readiness rule's label is
unrecoverable or its resolved `source` property is absent. When `elephant next
--json` runs, the task does not appear in `next`. It appears once in
`withheld` with `missing-ready-rule` or `missing-ready-source`.

### TEST-605: Pair Explanation with Rule Metadata

Validates: [[SPEC-006-elephant-next#REQ-605]],
[[SPEC-006-elephant-next#NFR-601]], and
[[SPEC-006-elephant-next#CON-601]].

Given the fixture from [[SPEC-006-elephant-next#TEST-601]], execute the
candidate's `explain` token array and its `describe` token array without
modification. `explain` proves `(ready models)` through `r-ready`; the
`describe --json` result contains that label's resolved `meta.source`.
Running `next --json` twice produces byte-identical sorted arrays.

### TEST-606: Read-Only Scope Invariant

Validates: [[SPEC-006-elephant-next#REQ-602]] and
[[SPEC-006-elephant-next#CON-602]].

Given a fixture theory and local identity, record the corpus entry ids,
membership record, local identity files, and daemon network counters. After
running `elephant next --json`, every recorded value is unchanged and no
producer or sync invocation occurred.

### TEST-607: Reject the Retired Task Surface and Plan Paths

Validates: [[SPEC-006-elephant-next#REQ-603]] and
[[SPEC-006-elephant-next#CON-601]].

Given a valid local `plan.spl` path, `elephant task next plan.spl` and
`elephant next plan.spl` both fail recognition before theory loading. The sole
accepted selection form is the existing `-t THEORY` selector.

### TEST-608: Return Idle Successfully

Validates: [[SPEC-006-elephant-next#REQ-606]] and
[[SPEC-006-elephant-next#CON-601]].

Given a readable theory with no complete eligible task, `elephant next --json`
exits zero, emits valid JSON with `next: []`, and does not emit an error object.

### TEST-609: Do Not Re-run the Reasoner

Validates: [[SPEC-006-elephant-next#NFR-601]] and
[[SPEC-006-elephant-next#CON-602]].

Given an instrumented closure fixture, one `elephant next --json` invocation
causes exactly one closure evaluation and no abduction evaluation. Recovering
each candidate's readiness-rule label performs no additional derivation.
Reordered admitted Entries yield byte-identical JSON.

### TEST-610: Ignore Near-Miss Predicate Shapes

Validates: [[SPEC-006-elephant-next#REQ-604]] and
[[SPEC-006-elephant-next#CON-601]].

Given a theory containing `(task alpha beta)` at arity 2, a `(ready 42)` whose
argument is an integer term, and a legacy flat `ready-legacy` atom, none of
these produces a candidate or a withheld entry. A valid `(task gamma)` in the
same theory still projects normally, showing the projection narrowed rather
than failed.

### TEST-611: Emit the Template Rule Label, Not the Grounded Instance

Validates: [[SPEC-006-elephant-next#REQ-605]] and
[[SPEC-006-elephant-next#CON-601]].

Given a theory whose quantified `r-ready` rule grounds several instances,
every candidate reports `ready_rule` as `r-ready` and never as a
`r-ready_<n>` grounded instance name. Executing the emitted `describe` token
array resolves a non-empty `meta.source`. A theory declaring both `r-ready`
and `r-ready-hotfix` attributes each candidate to the rule that derived its
own `(ready X)`.

### TEST-612: Envelope Provenance Is Not a Readiness Citation

Validates: [[SPEC-006-elephant-next#REQ-604]] and
[[SPEC-006-elephant-next#CON-601]].

Given a ready, fully documented task whose readiness rule carries no authored
`(meta … (source …))`, the rule's resolved `source` is nonetheless the
claims-attached `agent:<did>` provenance of its signer. The task does not
appear in `next`; it appears once in `withheld` with `missing-ready-source`.
A second theory that adds an authored citation to the same rule offers the
task, with `source` reporting the citation rather than the signer.

### TEST-613: Template Recovery Prefers an Existing Rule

Validates: [[SPEC-006-elephant-next#REQ-605]].

Given a theory containing a rule whose own label ends in a digit run —
`phase_2` — alongside `r-ready`, resolution returns `phase_2` unchanged,
resolves a grounded `phase_2_7` to `phase_2`, resolves `r-ready_2` to
`r-ready`, and returns nothing for a label no prefix of which names a rule.
A task whose readiness is derived by `phase_2` reports that rule, never
`phase`.

## Observability

### OBS-601: Task Discovery Projection

`elephant next` emits one structured trace event containing the resolved
theory id, closure fingerprint, candidate count, withheld count grouped by
reason, and projection duration. It MUST NOT include task description text,
acceptance text, source text, corpus payloads, or identity secret material.

The exclusion of description and acceptance text is load-bearing under
[[SPEC-006-elephant-next#ADR-604]], because that prose is now an ordinary
conclusion and so is reachable at the point the event is built.

This signal makes a missing or unexpectedly empty task selection diagnosable
without turning a read query into a second task log.

## Quality Gates

- [x] [[SPEC-006-elephant-next#REQ-601]] through
  [[SPEC-006-elephant-next#REQ-606]] each link to applicable tests and
  [[SPEC-006-elephant-next#OBS-601]].
- [x] [[SPEC-006-elephant-next#CON-601]] has a full recogniser contract and
  rejects plan paths before theory loading — `Next` is a fieldless clap
  variant, so recognition fails before `Paths::resolve`.
- [x] [[SPEC-006-elephant-next#CON-602]] has a pure-core implementation
  (`src/core/next.rs`, importing only spindle value types and the closure) and
  a passing scope-invariant test.
- [x] A fresh-context reviewer has checked that this does not reverse
  [[SPEC-003-elephant-tasks#ADR-206]] — reviewed against the specification
  alone with a defect-finding mandate; five findings, all triaged below.
- [x] The `explain` / `describe --json` metadata pairing is verified by
  [[SPEC-006-elephant-next#TEST-605]], executing the emitted token arrays
  unmodified.
- [x] [[SPEC-006-elephant-next#ADR-604]] is consistent with
  [[SPEC-005-elephant-vocabulary#ADR-402]] and introduces no new suffix
  resolution — template recovery is existence-driven and mirrors spindle.
- [ ] [[SPEC-006-elephant-next#OPEN-601]] — still open. It governs `status`
  display, not this projection, so it did not block implementation; it does
  block calling the ergonomics settled.
- [ ] [[SPEC-006-elephant-next#REQ-602]] is satisfied by `next` itself, but
  the shared read path it inherits is not yet clean — see the MLS-lane gate
  below.

## Gate Evidence Record

```yaml
phase: 1
gates:
  - gate: "New specification links resolve"
    mechanism: "zetl -d specs check --dead-links"
    result: pass
    evidence: >-
      Run 2026-08-09 against v0.2.0. SPEC-006 contributes 14 dead links,
      byte-identical in target and count to the v0.1.0 baseline: the glossary
      terms Commitment, SPL, Speech Act, Logical Theory, hence, and the
      users/agent/* profile paths, all of which resolve outside -d specs and
      are dead for every sibling spec too. Every SPEC-006 and SPEC-005 anchor
      introduced by this revision — ADR-604, OPEN-601, TEST-610, TEST-611,
      SPEC-005#ADR-402, SPEC-005#CON-401, SPEC-005#REQ-406 — resolves.
  - gate: "No reversal of retired task layer"
    mechanism: "reviewer: fresh-context architectural review"
    result: unverified
    evidence: "Not yet requested."
  - gate: "Theory-derived task selection is testable"
    mechanism: "TEST-601 through TEST-611 fixture plan"
    result: unverified
    evidence: "Implementation has not started."
  - gate: "Prose survives as a predicate argument term"
    mechanism: "spindle parser harness + end-to-end elephant assert/status"
    result: pass
    evidence: >-
      Run 2026-08-09 on elephant 0.1.6 (debug). Parser: (given (task-description
      models "Design the data model")) yields task-description/2 with args
      [Symbol("models"), Symbol("Design the data model")] — interior spaces
      intact, no truncation at the space. End to end: the same assert through
      the real producer path is accepted (s-da88b8f918c7a066) and `status`
      reports it as a +D conclusion. (ready models) is +d via the quantified
      rule, confirming prose facts and readiness coexist.
  - gate: "Readiness rule label needs no second reasoner pass"
    mechanism: "single reason() call, inspect Conclusion.rule_label"
    result: pass
    evidence: >-
      Run 2026-08-09. One spindle reason() call over a 3-task fixture produced
      26 conclusions, each positive (ready ?x) carrying a rule_label, with no
      explanation::explain call anywhere in the harness. Grounded instance
      names observed as r-ready_2, r-ready_3, r-ready-hotfix_1, confirming the
      {template}_{n} rename REQ-605 guards against is real rather than
      hypothetical; stripping the trailing _<digits> run recovers r-ready and
      r-ready-hotfix, each resolving its own meta.source. A task derivable only
      by r-ready-hotfix is attributed to that rule, satisfying TEST-611.
  - gate: "REQ-605 receipt pair resolves end to end"
    mechanism: "elephant explain / describe --json on a live theory"
    result: pass
    evidence: >-
      Run 2026-08-09. `explain '(ready models)'` proves it via defeasible rule
      r-ready — note explain already reports the template label, not the
      grounded instance. `describe r-ready --json` returns
      meta.source = SPEC-006-elephant-next#TEST-601. `promise '(completed
      models)'` is accepted and `commitments --json` reports goal
      "(completed models)" in state outstanding, exercising REQ-601's
      exclusion condition.
  - gate: "Implementation conforms (unit + CLI acceptance)"
    mechanism: "cargo test --lib && scripts/spec006-acceptance.sh"
    result: pass
    evidence: >-
      169 library tests pass, of which 18 are the projection's own in
      src/core/next.rs covering TEST-601..604 and TEST-609..613. The CLI-level
      checks that need a real binary and store — TEST-605, TEST-606, TEST-607,
      TEST-608, NFR-601 determinism, and the OBS-601 no-prose assertions — run
      from scripts/spec006-acceptance.sh (make acceptance), all passing.
      clippy -D warnings clean.
  - gate: "Adversarial review (Constitutional Principle 12)"
    mechanism: "fresh-context agent, specification + deliverable only"
    result: pass-with-findings
    evidence: >-
      Five findings, each empirically demonstrated. Fixed: (1) literals were
      built by string interpolation, so a quoted identifier such as "my task"
      rendered as the two-argument (completed my task) and an outstanding
      commitment stopped blocking it — now built through Literal::to_spl();
      (2) a modal (must (ready X)) was matched as plain readiness, offering
      work the theory does not derive — mode/temporal now rejected; (3) the
      human "take" line was a copy-pasteable shell command embedding corpus
      text, so a task name containing a quote was an injection vector — no
      longer shell-quoted, and control characters are replaced; (4) template
      recovery over-stripped a rule genuinely named phase_2, and the old unit
      test had encoded that bug as intended — replaced by existence-driven
      resolution. Not fixed, recorded below: (5) the shared read path.
      Review also identified six weak tests; TEST-602 now commits its goal
      non-canonically and reason precedence is checked pairwise.
  - gate: "REQ-602 holds for the whole read path"
    mechanism: "call-graph trace through queries::view"
    result: fail
    evidence: >-
      `next` itself appends nothing, but view() calls e2ee::process_mls_lane,
      which writes mls/lane.cursor and may rewrite the keybook. That is not an
      Entry append, sync, or producer call, so CON-602's narrower prohibition
      holds and TEST-606 passes — but only vacuously, because the fixture's
      single-member lane is empty and the call short-circuits. REQ-602's
      broader "local identity state" clause is therefore unverified for a
      theory with pending lane traffic. Inherited from every read command, so
      the fix belongs to the SPEC-001 read path rather than here.
```

## Status

**Implemented**, with two carried items.

The command, its pure core, its tests, and its documentation are in place and
the gates above record their evidence. Two things are deliberately not closed:
[[SPEC-006-elephant-next#OPEN-601]], which governs `status` display rather
than this projection; and the MLS-lane write in the shared read path, which
predates this work and which no change scoped to `next` can fix.

## Changelog

<details>
<summary>Revision history</summary>

- 0.3.0 — implemented. `elephant next` ships as a pure projection
  (`src/core/next.rs`) behind a fieldless CLI variant. Adds
  [[SPEC-006-elephant-next#TEST-612]] (envelope provenance is not a citation)
  and [[SPEC-006-elephant-next#TEST-613]] (existence-driven template
  recovery). Normative additions found by building it: `task`/`ready`/
  `completed` are reserved built-in *control* vocabulary and are neither
  declared nor declarable, and `kind` admits only `evidence`/`state`/
  `discovery`, so 0.2.0's `(predicate task …)` / `(kind doc)` examples were
  both invalid; a rule's auto-attached `agent:` provenance does not satisfy
  the readiness-source gate; template recovery is existence-driven; modal and
  temporally-bounded readiness is excluded; and every emitted literal is
  rendered through `Literal::to_spl()` rather than string interpolation.
  Carried: [[SPEC-006-elephant-next#OPEN-601]], and an MLS-lane write in the
  shared read path that predates this change.
- 0.2.0 — identify tasks by predicate argument (`(task ?x)`, `(ready ?x)`,
  `(completed ?x)`) instead of the flat `task-X` atom family, per
  [[SPEC-006-elephant-next#ADR-604]]. Replaces the `task-id` ABNF with a
  predicate-symbol recogniser table; collapses N per-task `r-ready-X` rules to
  one quantified `r-ready`; moves per-task prose from `meta` to the
  `task-description/2` and `task-acceptance/2` facts, because spindle's
  `MetaTarget` has no per-instance form. Adds TEST-610 (near-miss shapes) and
  TEST-611 (template rule label). Closes 0.1.0's deferred parameterised-task
  question and opens [[SPEC-006-elephant-next#OPEN-601]] in its place.
  Verified against elephant 0.1.6, which corrected two drafting errors: goal
  equality reuses canonical `to_spl()` comparison rather than the spindle
  `Literal` equality first claimed, and machine operands are rendered by
  `to_spl()` rather than `closure::literal_display`, which strips the outer
  parentheses for human output.
- 0.1.0 — initial native, read-only `elephant next` proposal. It restores
  theory-derived task discovery while preserving SPEC-003's retirement of the
  hence-compatible task lifecycle surface.
</details>
