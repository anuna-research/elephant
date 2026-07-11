---
id: SPEC-003
title: elephant tasks — the hence-successor coordination layer
version: 0.2.0
status: implemented
date: 2026-07-11
last-updated: 2026-07-11
audience: agent, human reviewer
---

# SPEC-003 — elephant tasks

## Orientation

Intent: elephant-3000 is [[hence]]'s successor. This spec carries hence's
task-coordination surface and its proven SPL lifecycle semantics onto the
elephant substrate: a plan is no longer a local `.spl` file appended to by
one machine — it is a [[Logical Theory]] shared over
[[SPEC-002-elephant-p2p|p2p sync]], where every lifecycle action is a
signed [[Speech Act]].

Metaphor: same game, bigger table — the SPL vocabulary hence agents
already speak is unchanged; the file became a corpus, the `:at` stamp
became a signature, and the plan travels.

Structure:

```
  hence 0.7 surface           elephant-3000 substrate
  ─────────────────           ───────────────────────
  plan.spl (append file)  →   theory corpus (Entries, SPEC-001)
  (claims agent:x :at …)  →   claims derived from signed envelope (ADR-012)
  task claim/complete/…   →   assert Entries carrying the SAME chain SPL
  plan board / task next  →   same closure-derived states, same JSON
  hence.run / libp2p      →   SPAKE2 join + pkarr + iroh (SPEC-002)
```

Decisions: [[#ADR-201]] preserve hence lifecycle SPL verbatim ·
[[#ADR-202]] bundles = batched Entries, inert-if-partial ·
[[#ADR-205]] one flat verb surface (supersedes [[#ADR-203]] alias groups) ·
[[#ADR-204]] LLM-orchestration features deferred.

Load-bearing: [[#REQ-201]] claim chain · [[#REQ-203]] complete ·
[[#REQ-205]] board states · [[#REQ-206]] next.

Open: `agent spawn`/`watch` pool on theories ([[#ADR-204]], owner HOC) ·
`plan translate/review/decompose` headless-LLM commands ([[#ADR-204]]).

Detail: [[SPEC-001-elephant-core]] (substrate),
[[SPEC-002-elephant-p2p]] (sync), hence 0.7 sources and its
`hence-v2-upgrade-plan` (the in-place upgrade this project supersedes).

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD
NOT, RECOMMENDED, MAY, and OPTIONAL in this document are to be
interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only
when, they appear in all capitals.

## 1. Compatibility contract with hence

The SPL vocabulary is frozen exactly as hence 0.7 emits it (this was the
declared v2 freeze; we honour it):

- task declaration `(given task-X)` + `(meta task-X (description …)
  (acceptance …))`; roots `(given no-deps-X)`
- readiness `(normally r-ready-X <deps> ready-X)` where dependency edges
  are `completed-<dep>` literals in readiness-rule bodies
- assignment `(normally r-assign-X (and ready-X agent-N-available)
  assign-to-X-N)`
- lifecycle chain-cancellation pattern with versioned literals
  (`claim-vN-X`, `state-claimed-vN-X`, `claimed-X`; `block`/`unblock`
  with `bl-`/`ubl-` labels; counters negate the intermediate and the
  head with `prefer` over the prior chain rules)
- completion `(given completed-X)`
- discovery vocabulary `discovered- decided- blocked-by- requires-
  verified- failed- finding- approach- insight- partial-`
- board state precedence: completed → blocked → upstream-blocked →
  decomposed → claimed → ready → backlog

What changes (and only this): the *carrier*. hence wrote
`(claims agent:x :at …)` wrappers into a file; elephant signs each
statement into an Entry and the closure pipeline reconstructs the claims
wrapper from the verified envelope
([[SPEC-001-elephant-core#ADR-012]]) — so attribution becomes
cryptographic where it used to be honorific, with the same SPL reaching
the reasoner.

## 2. Requirements

#### REQ-201: Claim

`elephant claim <task> -t <theory> [--force]` SHALL verify the task
exists and `ready-<task>` is defeasibly provable (unless `--force`),
then append the hence claim bundle — for next free version N and prior
unclaim version M when present:

```lisp
(given claim-vN-<task>)
(normally r-cl-state-vN-<task> claim-vN-<task> state-claimed-vN-<task>)
(normally r-cl-chain-vN-<task> state-claimed-vN-<task> claimed-<task>)
;; iff M exists:
(normally r-cl-cancel-vM-<task> claim-vN-<task> (not state-unclaimed-vM-<task>))
(prefer r-cl-cancel-vM-<task> r-ucl-state-vM-<task>)
(prefer r-cl-chain-vN-<task> r-ucl-unclaimed-vM-<task>)
```

plus hence's failure-propagation rules for each downstream dependent
(`r-propagate-<dep>-from-<task>-{failed,permanently-failed,stale-vN,timeout-vN,blocked}`
→ `upstream-blocked-<dep>`, with transitive `-blocked` cascades),
deduplicated against labels already in the theory. Claiming an
already-claimed task is idempotent (no-op, exit 0, notice).

Trace: [[#TEST-201]] · [[#ADR-201]] · [[#ADR-202]]

#### REQ-202: Unclaim

`elephant unclaim <task> -t <theory>` SHALL refuse when
`completed-<task>` holds, be idempotent when not claimed, and otherwise
append the counter bundle for next version N against prior claim
version P:

```lisp
(given unclaim-vN-<task>)
(normally r-ucl-state-vN-<task> unclaim-vN-<task> state-unclaimed-vN-<task>)
(normally r-ucl-cancel-vP-<task> unclaim-vN-<task> (not state-claimed-vP-<task>))
(prefer r-ucl-cancel-vP-<task> r-cl-state-vP-<task>)
(normally r-ucl-unclaimed-vN-<task> unclaim-vN-<task> (not claimed-<task>))
(prefer r-ucl-unclaimed-vN-<task> r-cl-chain-vP-<task>)
```

Trace: [[#TEST-202]]

#### REQ-203: Complete

`elephant complete <task> -t <theory>` SHALL verify the task exists
(listing available tasks on miss), be idempotent when already complete,
and append `(given completed-<task>)`. When completion makes every
declared task Done, the CLI SHALL report plan completion.

Trace: [[#TEST-203]]

#### REQ-204: Block / unblock

`elephant block <task> '<reason>' -t <theory>` SHALL append the
`bl-` chain bundle (mirror of [[#REQ-201]] with block/blocked vocabulary)
AND, fixing hence 0.7's silent reason drop, a
`(given (blocked-by <task> "<reason>"))` fact carrying the reason into
the theory. `unblock` mirrors [[#REQ-202]] with `ubl-` labels.

Trace: [[#TEST-204]]

#### REQ-205: Board

`elephant board -t <theory> [--agent A] [--json]` SHALL render
tasks bucketed exactly by hence's precedence (completed → blocked →
upstream-blocked → decomposed → claimed/in-progress → ready → backlog)
from the closure, with hence's JSON shape
(`{"backlog":[…],"ready":[…],"in_progress":[…],"decomposed":[…],
"blocked":[…],"done":[…]}`, items `{"task","assignee"?,"block_type"?}`).

Trace: [[#TEST-205]]

#### REQ-206: Next

`elephant next -t <theory> [--agent A] [--json]` SHALL list
assignments for tasks that are ready ∧ ¬(completed ∨ claimed ∨ blocked ∨
upstream-blocked ∨ decomposed), filtered to the agent when given, with
hence's fallback (no ready task → all assignments unfiltered) and a
ready-to-paste claim command per item.

Trace: [[#TEST-206]]

#### REQ-207: Task-annotated assert

`elephant assert '<spl>' -t <theory> [--task X]` SHALL behave as
[[SPEC-001-elephant-core#REQ-005]] (it is the same command surface hence
agents already script against; `--task` is annotation only). The
`task assert` group spelling from v0.1.0 is removed with the alias
groups ([[#ADR-205]]) — it was byte-for-byte the same implementation.

Trace: [[SPEC-001-elephant-core#TEST-005]]

#### REQ-208: Describe and trace

`elephant describe <label…>` (rule/fact definition + provenance + meta)
and `elephant trace` (full conclusion dump with firing order) SHALL be
provided as flat commands alongside the
[[SPEC-001-elephant-core#REQ-011]]–[[SPEC-001-elephant-core#REQ-014]]
query verbs (`explain`, `why-not`, `require`, `what-if`), which they
extend. The `query` group spelling from v0.1.0 is removed
([[#ADR-205]]).

Trace: [[#TEST-208]]

#### REQ-209: Plan info and template

`elephant theory create <name> --template plan` SHALL seed the new
theory with hence's default plan skeleton (`(meta plan …)` + example
task/readiness/assignment structure) as genesis-adjacent Entries;
`elephant info -t <theory>` SHALL render the `(meta plan …)` block.

Trace: [[#TEST-209]]

#### REQ-210: Agent availability

`elephant join-as <agent-name> -t <theory>` SHALL assert
`(given agent-<name>-available)` so assignment rules can bind the local
agent, using the signer's registered name by default.

Trace: [[#TEST-210]]

### Non-functional

#### NFR-201: A hence 0.7 plan body (the SPL forms of a plan.spl,
claims wrappers dropped) SHALL be importable via repeated
`assert` with identical closure results for board/next/status on a
10-task reference plan. // migration oracle, verified by TEST-211

## 3. Architecture decisions

#### ADR-201: Preserve hence lifecycle SPL verbatim

**Decision.** The chain-cancellation bundles are byte-compatible with
hence 0.7's `generate_action_spl`/`generate_counter_spl` output (modulo
the claims wrapper, which moves to the envelope layer).

**Context.** The pattern (fact → versioned intermediate → head, counters
negate via superiority) exists because `given` facts cannot be defeated
in SDL; it is battle-tested in hence and its semantics are what existing
agents and skills (the `/hence` skill, hooks, JSON consumers) rely on.
hence's own v2 plan froze this vocabulary; we inherit the freeze.

**Trade-offs.** (+) drop-in mental model and scripts; migration is
mechanical ([[#NFR-201]]). (−) versioned-literal proliferation is
verbose in the log; acceptable — the corpus is append-only anyway.

#### ADR-202: Bundles are batched Entries; partial bundles are inert

A lifecycle action emits one Entry per SPL statement, appended in a
single local Loro commit (atomic locally, one sync unit in practice).
If a partition ever delivers a strict subset, the chain pattern
degrades safely: a `claim-vN` fact without its chain rules derives
nothing. No cross-Entry transaction machinery is introduced.
// SIMPLIFY: no atomic multi-entry transactions; ceiling: an adversarial
// member replaying selective sub-bundles — mitigated by signatures and
// audit, revisit with SPEC-002 revocation (trace: this ADR).

#### ADR-203: Command surface — SUPERSEDED by [[#ADR-205]] (v0.2.0)

Canonical groups mirror hence: `plan {board,status,info,validate,
summary}`, `task {next,claim,unclaim,complete,block,unblock,assert}`,
`query {explain,why-not,require,what-if,describe,trace}`. The flat
spellings in [[SPEC-001-elephant-core]] (`elephant status`,
`elephant why-not …`) are retained as aliases; `plan status` and flat
`status` are one implementation. `-f plan.spl` file mode is NOT carried
over — hence remains available for file-local work; elephant's
offline-first store ([[SPEC-001-elephant-core#REQ-020]]) covers the
"no network" case with strictly more capability.

The `-f plan.spl` exclusion and the file-mode reasoning stand; only the
dual-spelling decision is reversed — see [[#ADR-205]].

#### ADR-205: One flat verb surface (supersedes ADR-203's alias groups)

**Decision.** The `plan`, `task`, and `query` command groups are
removed. Every read and write verb has exactly one spelling, flat:
producers `assert · retract · promise · request · concede`; reads
`status · explain · why-not · require · what-if · commitments · log ·
watch · describe · trace`; coordination `board · info · join-as · next
· claim · unclaim · complete · block · unblock`. Noun groups survive
only for lifecycle administration where the noun disambiguates the
object: `id {create,whoami}`, `theory {create,list,invite,join,members,
remove}`, `daemon {start,run,status,stop}`.

**Context.** [[#ADR-203]] kept the hence group spellings for drop-in
compatibility, making `plan status` and flat `status` "one
implementation". In practice the compatibility surface that matters is
frozen elsewhere: the SPL vocabulary ([[#ADR-201]]) and the JSON output
shapes ([[#REQ-205]], [[SPEC-001-elephant-core]] CON-004) — not the
argv spelling. No external consumer scripts the hence CLI spelling
against elephant: this project supersedes the in-place hence-v2
upgrade, so there is no installed base to migrate, and hence itself
remains available for file-local work. Meanwhile the dual surface had a
real, recurring cost: every aliased verb appeared twice in `--help`,
docs and examples split between spellings, tab-completion doubled, and
each alias pair was an extra `Command::X | Command::Group(...)` arm in
dispatch. Two spellings for one action also violates the spirit of
"one parser per language" ([[PROTO-001]] LangSec principle 5) applied
to the human interface: redundant surface invites divergence.

**Trade-offs.** (+) one canonical spelling per action — help,
docs, completion, and agent-facing examples are unambiguous; dispatch
shrinks (five alias arms and three subcommand enums deleted); the flat
speech-act verbs (`elephant promise …`) read as the product's identity.
(−) anyone with hence muscle memory types `elephant task claim x` and
gets a usage error — mitigated by clap's suggestion machinery and by
the error being immediate and loud rather than a silent divergence.
(−) ~24 flat leaves in `--help` — kept scannable by declaration order
grouping the help into admin/produce/read/coordinate runs.

**Deferred.** The `theory` group still takes the theory as a
positional argument (`elephant theory members <theory>`) while every
flat verb uses global `-t`. Aligning that is a separate, smaller
decision — deferred, owner HOC, tracked here.

#### ADR-204: LLM-orchestration features deferred

`agent spawn`/`watch` (worktrees, PTY supervision, leases, evaluator
verdicts), `plan translate/review/decompose`, `learn`, sessions, and
hooks are NOT in v0.1. The substrate is deliberately sufficient for
them (leases are `(during …)` facts; supervisor/evaluator are just
signers with their own DIDs — a cleaner fit than hence's three local
keypairs), and they arrive as a follow-up spec once the core is proven.
// SIMPLIFY: no spawn/watch in v0.1; ceiling: autonomous fleet use;
// upgrade path: SPEC-004 porting hence spawn.rs semantics (trace: this ADR).

## 4. Test specification

| TEST | Validates | Positive | Negative-input | Negative-output |
|---|---|---|---|---|
| TEST-201 | REQ-201 | claim on ready task → claimed-X +d; bundle matches hence oracle byte-for-byte (modulo wrapper) | claim unready w/o --force → refused | propagation rules missing for dependents → fail |
| TEST-202 | REQ-202 | claim→unclaim → claimed-X not provable | unclaim completed task → refused | prior chain still wins → fail |
| TEST-203 | REQ-203 | complete → Done; all-done notice | unknown task → exit 8 + list | — |
| TEST-204 | REQ-204 | block → Blocked + reason fact present | block completed → refused | reason dropped → fail |
| TEST-205 | REQ-205 | 7-state fixture renders hence JSON shape | — | precedence order wrong → fail |
| TEST-206 | REQ-206 | ready+assigned filtering; fallback when none ready | — | claimed task listed as next → fail |
| TEST-208 | REQ-208 | describe shows rule+source; trace complete | unknown label → clean miss; removed group spellings (`query`, `task`, `plan`) → usage error | — |
| TEST-209 | REQ-209 | template seeds meta plan; info renders | — | — |
| TEST-210 | REQ-210 | join-as → assignments bind | — | — |
| TEST-211 | NFR-201 | port hence's own demo plan; board/next/status equal hence 0.7 output on same theory | — | any bucket diff → fail |

TEST-211 is the migration oracle: run hence 0.7 against a fixture
plan.spl, run elephant against the imported theory, diff the JSON.

## Changelog

<details>
<summary>Revision history</summary>

- 0.2.0 — implemented: flatten the command surface: [[#ADR-205]] supersedes
  [[#ADR-203]]'s alias groups; `plan`/`task`/`query` groups removed,
  their verbs promoted to single flat spellings; REQ-201..210 and
  NFR-201 restated with the flat spellings (semantics unchanged).

- 0.1.0 — implemented: hence-byte-compatible lifecycle bundles, board/next/plan, migration oracle diffs clean against live hence 0.7.

- 0.1.0 — initial; authored after the stakeholder directive that
  elephant-3000 succeeds hence, from the hence 0.7 source survey and the
  superseded hence-v2 upgrade plan.
</details>
