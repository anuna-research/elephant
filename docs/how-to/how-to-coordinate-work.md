---
title: How to coordinate work from a theory
mode: how-to
---

# How to coordinate work from a theory

This guide turns documented readiness into a promise and evidence-backed completion.
It assumes an identity and theory from [[first-theory]].
For peer work, complete [[how-to-share-a-theory]].

## Inspect the vocabulary

Inspect the theory's [[Predicate Family|predicate families]] before introducing new terms:

```sh
elephant vocab -t release --json
elephant commitments -t release --json
```

For a new evidence predicate, document its arguments and intended asserter:

```sh
elephant define ci-green/1 --arg task:symbol \
    --desc "CI pipeline green for task ?t" --kind evidence --asserter role:ci -t release
elephant assert '(normally r-verified (and (ci-green ?x) (review-approved ?x)) (verified ?x))' -t release
```

Built-in families such as `task`, `ready`, `completed`, and `verified` are reserved at every arity.
Use [[SPEC-005-elephant-vocabulary]] for their contract.

## Supply actionable work

For existing tasks, skip to discovery below.
For a new task, supply its description, acceptance criterion, and readiness source:

```sh
elephant assert '(given (task models))' -t release
elephant assert '(given (task-description models "Design the data model"))' -t release
elephant assert '(given (task-acceptance models "Model contract tests pass and review approves"))' -t release
elephant assert '(given (prerequisite-met models))' -t release
elephant assert '(normally r-ready-models (and (task models) (prerequisite-met models)) (ready models))' -t release
elephant assert '(meta r-ready-models (source "how-to-coordinate-work#Supply actionable work"))' -t release
elephant assert '(normally r-complete-models (and (ci-green models) (review-approved models)) (completed models))' -t release
elephant assert '(meta r-complete-models (source "how-to-coordinate-work#Record acceptance evidence"))' -t release
```

These source strings identify this demonstration.
For project work, replace them with the governing requirement and test references.
A task identifier is an argument, such as `(task models)`, rather than a predicate suffix.

For shared prerequisites and provenance, a quantified rule can serve multiple tasks.
For distinct sources or dependencies, use separately labelled readiness rules.
Check their conclusions before offering the tasks to another agent.

## Discover and promise

Inspect the available work:

```sh
elephant next -t release --json
```

The `next` array contains documented, ready work without an outstanding completion promise.
The `withheld` array explains missing documentation, missing provenance, existing commitments, and completed tasks.
An empty candidate list alone does not establish that all work is complete.

Inspect the candidate's proof and source:

```sh
elephant explain '(ready models)' -t release
elephant describe r-ready-models -t release --json
```

Each candidate includes `commands` arrays for `explain`, `describe`, and `promise`.
Execute these as argument vectors, rather than shell strings assembled from theory text.
The query does not assign, promise, or sync.

When taking responsibility for the work, promise its goal:

```sh
elephant promise '(completed models)' -t release
```

A promise records responsibility; it does not provide an exclusive lock.
Check commitments before taking work that another agent can also see.

## Record acceptance evidence

Run the project's contract tests and review process.
Only after each check succeeds, assert the corresponding observed evidence:

```sh
elephant assert '(given (ci-green models))' -t release
elephant assert '(given (review-approved models))' -t release
```

Check completion and the promise state:

```sh
elephant explain '(completed models)' -t release
elephant commitments -t release --json
```

The completion rule requires both facts; the derived goal fulfils the promise.
If completion remains unsupported, inspect `why-not` before adding evidence.
`require` offers verified hypothetical remedies; its output does not establish that those facts occurred.

The readiness convention comes from [[SPEC-006-elephant-next]].
A signer's automatic `agent:` source records attribution, but does not satisfy the readiness citation requirement.

## Diagnose a predicate mismatch

For a daemon-served assertion, inspect its receipt for near-miss advice:

```sh
elephant assert '(given (review-approved m1))' -t release
elephant assert '(given (ci-green m2))' -t release
```

When the daemon cache is warm, a near-miss advisory can point from `m2` to the waiting `m1` witness.
The advisory is best-effort, absent in direct-store mode, and never blocks an assertion.
Its limits are defined in [[SPEC-005-elephant-vocabulary#NFR-402]].
