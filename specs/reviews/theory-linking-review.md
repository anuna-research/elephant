---
title: Theory linking design review and evidence
mode: reference
date: 2026-09-16
status: draft-review
---

# Theory linking design review and evidence

Deliverables: [[SPEC-007-theory-references]] and [[SPEC-008-theory-reports]].
This record distinguishes design checks from implementation verification and stakeholder approval.
The source request is the user's theory-linking discussion followed by `$anuna-dev to design this spec`.
The stakeholder selected `References + published reports (Recommended)` during review.
This validates the first-version scope; it does not constitute approval of every contract or security decision.

## Synthesis record

Author: primary Codex session, GPT-6-based identity supplied by the environment; exact runtime variant unavailable.
Inputs: stakeholder conversation, the anuna-dev protocol, existing specification contracts, and URI standards.
Model sampling parameters are environment-managed and were not independently recorded.
No private reasoning transcript is an acceptance artefact; this record preserves decisions, external findings, revisions, and check outputs.

Initial design: portable references plus destination-bound reports using existing signatures.
The capability split preserves logical isolation and avoids introducing Spindle network imports.
Initial draft snapshots are `evidence/theory-linking/draft-007-r0.txt` and `draft-008-r0.txt`, relative to the specification vault.
The contracts are drafts and do not describe shipped commands.

## Independent adversarial review

Reviewer: `/root/adversarial_spec_review`, model `gpt-5.6-sol`, fresh context.
Prompt: find concrete contradictions, security gaps, schema omissions, missing tests, and controls absent from Orientation.
Inputs: specifications and linked user/concept material only; no author conversation or reasoning.
This is cross-model review; independent training-family diversity is not established by the available tool metadata.

| Finding | Requirement attribution | Repair |
|---|---|---|
| Known keys were conflated with current membership | [[SPEC-008-theory-reports#REQ-702]] | Define known-key admission honestly; bridge authors explicitly choose reporters. |
| Publisher atom mapping was indirect | [[SPEC-008-theory-reports#REQ-705]] | State the injective full DID-tail mapping and test different local aliases. |
| Replication collisions lacked projection semantics | [[SPEC-008-theory-reports#REQ-702]] | Group distinct verified entries by receipt; ambiguous groups emit no premise. |
| CLI canonicalisation leaked into signed-field recognition | [[SPEC-007-theory-references#REQ-601]], [[SPEC-008-theory-reports#REQ-702]] | Require exact equality with canonical formatting inside signed payloads. |
| Read command time inputs were incomplete | [[SPEC-007-theory-references#NFR-601]] | Declare `--at`, one live time sample, and fixed-time byte checks. |
| Namespace reservation changed old theory semantics | [[SPEC-008-theory-reports#REQ-706]] | Gate projection and reservation on closure 3; keep old theories unchanged. |
| Retraction of colliding receipts was unspecified | [[SPEC-008-theory-reports#REQ-704]] | Apply E1 to all same-signer matches; retain ambiguity; recover with a new receipt. |
| Observation timestamp implied an unstated source cutoff | [[SPEC-008-theory-reports#REQ-701]] | Define evaluation over captured local input, not a source knowledge frontier. |
| Historical withdrawal lacked explicit tests | [[SPEC-008-theory-reports#REQ-704]] | Test later-learned withdrawals and fixed known inputs. |

Finding disposition is specification refinement, not a claim that product defects were fixed.
Second pass found a collision-output schema contradiction and an optional-newline recognition ambiguity.
Repair: use discriminated report/ordinary collision candidates and compare against precisely the allowed canonical JSON byte forms.
Attribution: [[SPEC-008-theory-reports#REQ-702]], [[SPEC-008-theory-reports#CON-701]], and [[SPEC-008-theory-reports#CON-702]].
The intermediate snapshot is `evidence/theory-linking/draft-008-r1.txt`.
The final bounded pass found no remaining blocking or significant contradiction in the repaired clauses and adjacent tests.
It identified minor schema presentation cleanup; the shared schema now explicitly includes `stored` and explains its reuse within collision candidates.
This verdict does not establish implementation correctness or replace the human security gate.

## Synthetic-user review

Reviewer: `/root/synthetic_user`, fresh context, GPT-6-based Codex identity; exact runtime variant unavailable.
Inputs: [[users/theory-linker/user]], [[users/theory-linker/happy-paths]], and the draft specifications; no implementation.
Prompt: simulate Alice and Bob through navigation, selected publication, and stale-evidence retirement; probe confusion and failure paths.

| Finding | Attribution | Repair or owner |
|---|---|---|
| Replicated reports had no direct inspection path | [[SPEC-008-theory-reports#REQ-703]] | Add `report show` for journal receipts. |
| Inspection schema omitted bridge fields | [[SPEC-008-theory-reports#CON-702]] | Define a shared complete ReportView. |
| Import success hid disabled reasoning | [[SPEC-008-theory-reports#REQ-706]] | Add adjacent mode cues and an explicit command sequence. |
| `supports` sounded inferential | [[SPEC-007-theory-references#REQ-605]] | Label human output as navigation only. |
| Recovery advice lacked next actions | [[SPEC-007-theory-references#CON-603]], [[SPEC-008-theory-reports#CON-702]] | Add local-access and unknown-key guidance without automatic trust changes. |
| Join and withdrawal delivery were compressed away | [[users/theory-linker/happy-paths]] | Require local statement/withdrawal receipt explicitly. |
| Terminal presentation depended on unstated affordances | [[SPEC-008-theory-reports#REQ-703]] | Require textual states, complete references, and returned machine premises. |
| Workflow completion time remained unmeasured | [[SPEC-008-theory-reports#Deferred work]] | Human maintainer owns a timed usability exercise before product acceptance; no latency claim is made. |

These are synthetic design observations, not user-study measurements.
The repeated walkthrough found no remaining blocking command-path gap under its stated closure-3 preconditions.
It retained nonblocking recovery-wording friction and unmeasured human usability as open work for the maintainer.

## Comprehension gate

First reviewer: `/root/comprehension_gate`, fresh context, GPT-6-based Codex identity.
Inputs: Orientation blocks only; no full specification or implementation.
Questions: private target access, backlink completeness, expiry, source changes, attestation, observe mode, and forged publisher facts.

The first report passed access, backlink, time-window, attestation, and mode questions.
It failed history/recomputation and forged-publisher details because the Orientation did not state enough to predict them.
Repair: add retained history, recomputation, observation-time meaning, canonical publisher mapping, and the protected predicate name to Orientation.
The repaired projection is tested again by a new fresh-context reader rather than changing the original verdict.
Second reviewer: `/root/comprehension_gate_r1`, fresh context, GPT-6 identity, Orientation-only inputs.
The repeated questions covered private links, expiry/history, testimony, modes, forged publishers, and existing closure-1 predicate compatibility.
The reader correctly predicted each case and identified its governing requirement.
Migration detail and exact diagnostic wording remain behind contract links, rather than being reproduced in Orientation.

## Mechanical evidence

Toolchain: `zetl 0.9.3`, `ar-crawl v1.0.29`, and the skill's supplied `usdd-lint.sh` and `usdd-count.sh`.
The actual specification vault is `specs/`, which contains its own `.zetl` directory.
The parent workspace scan excludes that nested vault; checks therefore explicitly use `-d specs`.
The baseline dead-link output precedes edits and remains in `evidence/theory-linking/zetl-baseline.json`.
Existing concept and cross-vault link debt is retained rather than removed to make a check green.
No existing specification, including the pre-existing untracked trust draft, is edited by this work.

`NO_COLOR=1` is incompatible with this installed zetl CLI's Boolean parsing; checks unset that environment variable.
The USDD lint's `/dev/stdout` access required an approved execution outside the sandbox.
The standards crawler required approved network access outside the sandbox.
These are observed execution conditions, not general claims about the tools.

URI standards: [[URI Profile]]; raw successful crawl output is retained beside the mechanical evidence.
Design-model command: `python3 specs/evidence/theory-linking/check-design.py`.
It checks new-spec references and bidirectional test attribution, then exercises a finite time-eligibility model.
The model accepts a valid expiry trace and rejects an inclusive-expiry mutant.
It is not Elephant implementation testing and does not establish source-proof correctness.
New-spec anchor checks and bidirectional test attribution pass in `evidence/theory-linking/design-check.json`.
The prose check outputs are in `evidence/theory-linking/prose-lint.json`.
Raw derived counts are in `evidence/theory-linking/count-007.json` and `count-008.json`; no hand-written declaration totals are substituted.
Vault-wide dead-link and CI checks remain failed because of baseline unresolved links.
The before/after comparison reports no new unresolved source/target pair; see `evidence/theory-linking/vault-delta.json`.
The gate record is `evidence/theory-linking/gates.json`.

## Remaining gates and next actors

- Human maintainer: validate the desired scope, URI spelling, selected disclosure, expiry workflow, and limit values.
- Human security reviewer: assess admission, publisher attribution, and migration before report implementation approval.
- Core maintainer: resolve the closure-2 implementation dependency and version-aware closure-3 rollout in the implementation plan.
- Implementation owner: author executable planning theory, red tests, parser fuzz targets, and product verification evidence before coding.
- Human operator: run a timed narrow-terminal QA walkthrough before product acceptance.

Status remains `draft`; no human approval, implementation readiness, deployment, or production verification is asserted.
