# Happy paths — Autonomous Agent

## HP-A1: CI step asserts an outcome

Preconditions: identity provisioned on the runner (key file); theory joined
previously by an operator; [[Daemon]] running on the host (or direct mode).
Steps:

1. `elephant assert 'ci-green' -t release-v3 --json` → exit 0, JSON with
   `receipt_id`, `signer`, `theory`, `spl_form`.
2. The assertion propagates to peers without further action.

Postconditions: signed claim in the corpus; peers' watches fire.
Failure modes: key file missing/unreadable → exit code for config error,
JSON error object on stdout is NOT emitted (errors go to stderr as JSON when
`--json`); malformed SPL → parse-error exit code, nothing signed.

## HP-A2: Agent discovers its next action

Preconditions: theory contains readiness/assignment rules (hence-style).
Steps:

1. `elephant status -t release-v3 --json` → array of `{literal, tag, ...}`.
2. `elephant require release-ready -t release-v3 --json` → abduced
   missing-fact set `["legal-signed"]`.
3. Agent decides it can produce `legal-signed`, does the work, asserts it.

Postconditions: stigmergic pickup — no orchestrator told the agent anything.
Failure modes: abduction budget exhausted → partial set flagged
`"complete": false` in JSON.

## HP-A3: Watch a literal and react

Preconditions: daemon running; agent process long-lived.
Steps:

1. `elephant watch release-ready -t release-v3 --json` → NDJSON stream; one
   line per tag change: `{"theory":..., "literal":..., "old":"-d", "new":"+d", ...}`.
2. On `+d`, the agent triggers its deployment routine and asserts
   `deployed` back into the theory.

Postconditions: reaction latency bounded by sync latency (NFR), not polling.
Failure modes: daemon restarts → stream reconnects or exits nonzero (agent
supervisor restarts it); literal never changes → stream stays open silently
(heartbeat comments optional per spec).

## HP-A4: One identity, many theories, one daemon

Preconditions: host daemon holds replicas of N theories; agent has one key.
Steps:

1. `elephant theory list --json` → all joined theories with ids and aliases.
2. Agent iterates, running `status --json` per theory, acting where it has
   work.

Postconditions: no cross-theory leakage; per-theory JSON is self-describing
(carries the theory id).
Failure modes: theory removed while iterating → per-call not-found exit code,
distinguishable from transport failure.
