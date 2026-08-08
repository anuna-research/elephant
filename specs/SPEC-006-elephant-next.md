---
id: SPEC-006
title: elephant next — theory-derived task discovery
version: 0.1.0
status: draft
date: 2026-08-08
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
  │                       task/rule meta                 promise command
  │                              │                              │
  └── explain ready-X ◀──────────┴── describe r-ready-X ─────────┘
```

Decisions: [[SPEC-006-elephant-next#ADR-601]] native read-only discovery ·
[[SPEC-006-elephant-next#ADR-602]] metadata is an eligibility gate ·
[[SPEC-006-elephant-next#ADR-603]] suggested commands are tokens, not shell
text.

Load-bearing: [[SPEC-006-elephant-next#REQ-601]] theory-derived candidates ·
[[SPEC-006-elephant-next#REQ-602]] no corpus mutation ·
[[SPEC-006-elephant-next#REQ-604]] complete metadata ·
[[SPEC-006-elephant-next#REQ-605]] proof and provenance.

Controls: [[SPEC-006-elephant-next#REQ-602]] `next` SHALL NOT append or
retract any [[Speech Act]] · [[SPEC-006-elephant-next#REQ-603]] it SHALL NOT
accept a `.spl` plan path, assign work, or schedule workers ·
[[SPEC-006-elephant-next#REQ-604]] incomplete task metadata SHALL be withheld,
never presented as actionable.

Open: The first release recognises the established flat `task-X` task
convention only. A parameterised task declaration needs a separate
specification after its per-instance metadata and promise-goal representation
are designed (owner: HOC).

Detail: [[users/agent/user]] · [[users/agent/happy-paths#HP-A2]] ·
[[SPEC-001-elephant-core#REQ-010]] ·
[[SPEC-003-elephant-tasks#ADR-206]] ·
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
or ignore the [[SPEC-005-elephant-vocabulary#REQ-402|task metadata]] that
explains the work and its acceptance criterion.

This specification restores only the query. `elephant next -t THEORY` is a
native projection over existing closure, commitment, and metadata views. It
does not restore hence's `.spl` carrier or its task lifecycle writers.

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
ready but its metadata is incomplete, the agent receives a repairable withheld
diagnostic and does not promise it.

## Task Convention

This specification defines the flat task convention consumed by `next`. It is
an opt-in projection of legal [[SPL]], not a new theory grammar or scheduler.
For a task identifier `X`, a candidate author supplies these exact forms:

```lisp
(given task-X)
(meta task-X
  (description "A concise statement of the work")
  (acceptance "The governing TEST-### acceptance criterion"))

(normally r-ready-X (and task-X prerequisite-X) ready-X)
(meta r-ready-X
  (source "SPEC-006-elephant-next#TEST-601"))
```

`meta` attaches documentation; it has no inferential effect. The `task-X`
fact and `r-ready-X` rule remain the semantic causes of task existence and
readiness. Every metadata property is exactly a `(key value)` pair: a bare
slash-form target or a property with several values is not this convention.

`description` and `acceptance` attach to the task label. `source` attaches to
the readiness-rule label. The existing metadata merge rule from
[[SPEC-005-elephant-vocabulary#REQ-402]] resolves each active property; this
specification consumes that resolved value and does not invent a second
metadata store.

## Requirements

### REQ-601: Theory-Derived Candidate Set

`elephant next` SHALL render a task identifier `X` as a candidate only when
all of these conditions hold:

- The selected theory positively derives `task-X` and `ready-X`.
- The theory does not positively derive `completed-X`.
- No outstanding [[Commitment]] has a goal structurally equal to `completed-X`.

The command does not infer readiness from file position, source order, agent
identity, an assignment literal, or an unproved rule head. Task authors put
holds, priorities, review requirements, and dependencies in the `ready-X`
rule. `next` projects the resulting closure without another decision system.

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

### REQ-604: Complete Metadata Is a Selection Gate

`elephant next` SHALL withhold a ready, non-terminal task from `next` when
any of these values is absent after metadata resolution:

- the task description;
- the task acceptance criterion;
- the standard `r-ready-X` rule; or
- that rule's source metadata.

The output SHALL identify the task and a stable withheld reason. A bare name
does not make a task actionable. The next agent needs the work, completion
criterion, and readiness source before making a promise.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-603]]
- [[SPEC-006-elephant-next#TEST-604]]
- [[SPEC-006-elephant-next#OBS-601]]

### REQ-605: Proof and Provenance Receipt

`elephant next --json` SHALL include the resolved description, acceptance,
ready literal, readiness-rule label, and source for every candidate. It SHALL
also include token arrays that invoke the existing `promise`, `explain`, and
`describe --json` commands for that exact task and theory.

[[SPEC-001-elephant-core#REQ-011|`explain`]] proves the readiness literal and
its firing rule. It does not render rule annotations. The returned
`describe --json` token array reads the rule's `meta.source`. This separation
preserves each existing query's meaning and makes metadata operational.

Trace:

- [[SPEC-006-elephant-next#CON-601]]
- [[SPEC-006-elephant-next#TEST-601]]
- [[SPEC-006-elephant-next#TEST-605]]
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
or shell fragment. The task identifiers it projects are parsed [[SPL]] atoms
from the admitted closure, not newly accepted user input.

The flat task convention is recognised over decoded atom and rule-label names.
`X` is a nonempty `task-id`. A closure value outside this grammar is ignored by
this projection and remains visible through `status` and `trace`.

```abnf
task-id        = lower *( lower / DIGIT / ( "-" ( lower / DIGIT ) ) )
lower          = %x61-7A
task-fact      = "task-" task-id
ready-fact     = "ready-" task-id
complete-fact  = "completed-" task-id
ready-rule     = "r-ready-" task-id
```

The `task-id` grammar deliberately rejects a leading, trailing, or doubled
hyphen. The implementation compares parsed atom names and rule labels; it
MUST NOT recover a task by substring search over raw SPL or output text.

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
      "ready_literal": "ready-models",
      "ready_rule": "r-ready-models",
      "source": "SPEC-006-elephant-next#TEST-601",
      "commands": {
        "promise": ["elephant", "promise", "completed-models", "-t", "<theory-id>"],
        "explain": ["elephant", "explain", "ready-models", "-t", "<theory-id>"],
        "describe": ["elephant", "describe", "r-ready-models", "-t", "<theory-id>", "--json"]
      }
    }
  ],
  "withheld": [
    { "task": "api", "reason": "missing-ready-source" }
  ]
}
```

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
  resolved metadata of that one theory.
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

### CON-602: Read-Only Projection Boundary

Interface: the implementation behind `elephant next`.

The pure core receives an already-admitted corpus, trust policy, evaluation
time, closure, commitment-state table, and resolved metadata map. It returns
the JSON-domain selection object described by [[SPEC-006-elephant-next#CON-601]].
The effectful shell loads those inputs and writes stdout or tracing output.

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
interpretation of Elephant's closure, commitment states, and `meta` entries.
Putting that interpretation in Circus or another process harness duplicates
the corpus projection or forces a lossy export boundary.

This is deliberately not the historical `hence task next plan.spl` port. The
command has no `.spl` file input, assignment filter, fallback assignment set,
claim command, or task-group alias. `next` is a new native query compatible
with [[SPEC-003-elephant-tasks#ADR-206]], not a reversal of it.

Simplicity Ladder placement: rung 4. The existing closure pipeline,
commitment reflection, metadata resolution, global theory selector, and JSON
renderer already provide all but the small pure selection projection. A new
scheduler, plan parser, task database, or process harness exceeds the first
sufficient rung.

### ADR-602: Metadata Is Required Before Work Is Suggested

The command will withhold incomplete tasks instead of returning bare names.
Task metadata does not affect inference, so this choice preserves the theory's
semantics while making its documentation actionable. The agent needs the
description and acceptance text before promising. The reviewer needs the
source-bearing readiness rule before assessing availability.

Alternative: display all ready tasks and show missing metadata as warnings.
Rejected: an agent loop will normally take the first available item, which
makes the metadata advice too easy to ignore. Withholding is reversible: the
theory author adds a valid `meta` assertion; no task state needs rewriting.

### ADR-603: Suggested Commands Are Token Arrays

The query will return command argument vectors instead of shell strings. This
gives a headless agent an exact next producer/query invocation without making
its JSON a code-execution channel. It also avoids shell quoting differences
and retains the user's independent approval before a `promise` changes shared
state.

Alternative: return only prose commands. Rejected: each client reconstructs
quoting and argument ordering, creating the vocabulary drift this feature
removes.

## Tests

### TEST-601: Render a Fully Described Ready Task

Validates: [[SPEC-006-elephant-next#REQ-601]],
[[SPEC-006-elephant-next#REQ-605]], and
[[SPEC-006-elephant-next#CON-601]].

Given a fixture theory with `task-models`, its task meta has valid description
and acceptance properties. The fixture has `r-ready-models`, valid rule source
metadata, and provable `ready-models`. When `elephant next -t fixture --json`
runs, it returns one `models` candidate with the metadata, source, and token
arrays from [[SPEC-006-elephant-next#CON-601]].

### TEST-602: Exclude Completed and Promised Tasks

Validates: [[SPEC-006-elephant-next#REQ-601]] and
[[SPEC-006-elephant-next#CON-601]].

Given two valid ready tasks, one has provable `completed-api`. Another has an
outstanding commitment with goal `completed-ui`. When `elephant next --json`
runs, neither appears in `next`. Each appears in `withheld` with its stable
reason.

### TEST-603: Withhold a Task Missing Task Metadata

Validates: [[SPEC-006-elephant-next#REQ-604]] and
[[SPEC-006-elephant-next#CON-601]].

Given provable `task-cache` and `ready-cache`, task metadata omits either
`description` or `acceptance`. When `elephant next --json` runs, `cache` does
not appear in `next`. It appears once in `withheld` with the matching missing
task-metadata reason.

### TEST-604: Withhold a Task Missing Readiness Provenance

Validates: [[SPEC-006-elephant-next#REQ-604]] and
[[SPEC-006-elephant-next#CON-601]].

Given a ready task with complete task metadata, the standard `r-ready-X` rule
or its resolved source property is absent. When `elephant next --json` runs,
the task does not appear in `next`. It appears once in `withheld` with
`missing-ready-rule` or `missing-ready-source`.

### TEST-605: Pair Explanation with Rule Metadata

Validates: [[SPEC-006-elephant-next#REQ-605]],
[[SPEC-006-elephant-next#NFR-601]], and
[[SPEC-006-elephant-next#CON-601]].

Given the fixture from [[SPEC-006-elephant-next#TEST-601]], execute the
candidate's `explain` token array and its `describe` token array without
modification. `explain` proves `ready-models` through `r-ready-models`; the
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
causes exactly one closure evaluation and no abduction evaluation. Reordered
admitted Entries yield byte-identical JSON.

## Observability

### OBS-601: Task Discovery Projection

`elephant next` emits one structured trace event containing the resolved
theory id, closure fingerprint, candidate count, withheld count grouped by
reason, and projection duration. It MUST NOT include task description,
acceptance text, source text, corpus payloads, or identity secret material.

This signal makes a missing or unexpectedly empty task selection diagnosable
without turning a read query into a second task log.

## Quality Gates

- [ ] [[SPEC-006-elephant-next#REQ-601]] through
  [[SPEC-006-elephant-next#REQ-606]] each link to applicable tests and
  [[SPEC-006-elephant-next#OBS-601]].
- [ ] [[SPEC-006-elephant-next#CON-601]] has a full recogniser contract and
  rejects plan paths before theory loading.
- [ ] [[SPEC-006-elephant-next#CON-602]] has a pure-core implementation and
  a passing scope-invariant test.
- [ ] A fresh-context reviewer has checked that this does not reverse
  [[SPEC-003-elephant-tasks#ADR-206]].
- [ ] The `explain` / `describe --json` metadata pairing is verified by
  [[SPEC-006-elephant-next#TEST-605]].

## Gate Evidence Record

```yaml
phase: 1
gates:
  - gate: "New specification links resolve"
    mechanism: "zetl -d specs check --dead-links"
    result: unverified
    evidence: "Run after SPEC-006 is added; record new versus baseline links."
  - gate: "No reversal of retired task layer"
    mechanism: "reviewer: fresh-context architectural review"
    result: unverified
    evidence: "Not yet requested."
  - gate: "Theory-derived task selection is testable"
    mechanism: "TEST-601 through TEST-609 fixture plan"
    result: unverified
    evidence: "Implementation has not started."
```

## Status

This specification is a draft. No implementation begins until a fresh-context
review confirms the task convention, commitment matching, and JSON contract.

## Changelog

<details>
<summary>Revision history</summary>

- 0.1.0 — initial native, read-only `elephant next` proposal. It restores
  theory-derived task discovery while preserving SPEC-003's retirement of the
  hence-compatible task lifecycle surface.
</details>
