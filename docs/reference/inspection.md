---
title: Inspection interfaces
mode: reference
---

# Inspection interfaces

These CLI interfaces expose conclusions, semantic fingerprints, and signed journal accounting.
The JSON contract is [CON-004](../../specs/SPEC-001-elephant-core.md#user-content-con-004-cli-json-contract).

## Conclusions

`status`, `trace`, and `what-if` expose proof tags and their plain-language `proof_state` names.
The mapping is versioned through `proof_state_map`.
Verbose text output also includes the plain-language state.

| Tag | Meaning |
|---|---|
| `+D` | Definitely provable |
| `+d` | Defeasibly provable |
| `-D` | Not definitely provable |
| `-d` | Not defeasibly provable |

A failure to prove a literal does not prove its negation.
Example output fields:

```json
{"literal":"release-ready","tag":"+d","proof_state":"defeasibly_provable","positive":true,"level":"defeasible"}
```

In source builds after `v0.1.7`, `reason --v2 --json` preserves argument types and all proof tags.
[Spindle reasoning in Elephant](../spindle-integration.md) documents the reasoning contracts and supported fragments.

## Closure fingerprints

| Command | Result |
|---|---|
| `closure fingerprint -t THEORY` | Digest using `elephant.closure.v1` |
| `status -t THEORY --fingerprint --json` | Digest embedded in status output |
| `closure compare A.json B.json` | Offline comparison of status files |

The digest hashes display strings and proof tags for non-membership conclusions.
It excludes membership, aliases, theory IDs, signer identities, and timestamps.
A changed proof tag changes the digest.
Identical fingerprints indicate equal represented conclusions, not equal journals or membership.
Temporal comparisons depend on a common evaluation time through `--at`.
The legacy fingerprint does not preserve the argument-type distinctions available in typed reasoning output.

`closure compare` exits `0` for a match and `10` for a clean mismatch.
Read or parse failures use the CLI's error exit codes.

## Journal accounting

`theory list` includes total journal entries.
`theory inspect THEORY` separates active, retracted, and setup/content assertions from retraction entries.
A total journal count is not an active-statement count.
The journal entry contract is [CON-002](../../specs/SPEC-001-elephant-core.md#user-content-con-002-entry-the-corpus-element).

`log` combines its filters with AND:

| Option | Filter |
|---|---|
| `--status` | `active`, `retracted`, `shadowed`, or `quarantined` |
| `--performative` | Speech-act kind, such as `assert` |
| `--signer` | Exact signer DID |
| `--sid` | Sentence ID or stable entry ID |
| `--retracts` | Retraction target |

Example invocations:

```sh
elephant theory inspect release
elephant log -t release --status active --performative assert
elephant log -t release --signer did:crdt:EXAMPLE --retracts s-EXAMPLE
```

The final invocation contains placeholder identifiers.
`show <sentence-id> -t THEORY --json` returns an individual entry and its envelope.
