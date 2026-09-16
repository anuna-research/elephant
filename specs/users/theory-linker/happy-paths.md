---
title: Theory linking design walkthroughs
mode: reference
---

# Theory linking design walkthroughs

Audience: [[users/theory-linker/user]].
These are proposed workflows for synthetic evaluation, not instructions for shipped commands.

## Connect evidence

Preconditions: Alice holds QA and Release; Bob holds Release; their local aliases differ.

1. Alice obtains QA's canonical theory reference from its local alias.
2. Alice records a signed `supports` link in Release pointing to a QA statement.
3. Bob lists the link and sees its author and stable target reference.
4. Bob follows it and receives `unavailable`; no join or fetch begins.
5. After independently joining QA and receiving the statement locally, Bob follows the same reference and sees the signed statement.

Postcondition: the reference survives alias differences; neither theory gains logical conclusions from the relationship.
Failure modes: malformed URI, ambiguous receipt, retracted evidence, unavailable source, and misleading backlink completeness.
Trace: [[SPEC-007-theory-references#Requirements]].

## Publish a QA result

Preconditions: Alice holds QA and closure-3 Release; Bob holds Release only; QA derives a build result.

1. Alice selects the ground literal, destination, expiry, and private output file.
2. Alice exports the signed report and sees that it is not yet imported.
3. Alice inspects the file to check the exact disclosure and reporter identity.
4. Alice imports it into Release; Bob receives it through existing Release replication.
5. Bob uses `report show` on the replicated receipt and sees `reporter-attested`, observation time, expiry, and the report reference.
6. Release authors an explicit bridge matching Alice, QA, the literal, and the intended tag.
7. Bob evaluates with reports enabled and sees the local conclusion's dependency on Alice's report.

Postcondition: Bob can authenticate the reporter without obtaining QA's private evidence.
Failure modes: wrong destination, unknown signer, tampered file, clock disagreement, expired report, and mistaken belief in independent source verification.
Trace: [[SPEC-008-theory-reports#REQ-701]], [[SPEC-008-theory-reports#REQ-702]], [[SPEC-008-theory-reports#REQ-703]], [[SPEC-008-theory-reports#REQ-705]].

## Retire stale evidence

Preconditions: Release uses an active imported QA report; QA later changes its conclusion.

1. Bob continues to see the historical observation until expiry or delivered withdrawal.
2. Alice retracts the old destination report if she no longer endorses its use.
3. After receiving the withdrawal locally, Bob's next Release evaluation removes that premise without asserting the opposite result.
4. Alice explicitly publishes a replacement if needed.
5. A rule pinned to the old receipt stays pinned; a recipient explicitly updates its acceptance rule.

Postcondition: expiry and withdrawal remain visible in history; replacement does not silently rewrite the recipient's policy.
Failure modes: offline withdrawal delivery, overlapping reports, forgotten expiry, cached watch output, and independent alternative support for readiness.
Trace: [[SPEC-008-theory-reports#REQ-704]], [[SPEC-008-theory-reports#REQ-706]].
