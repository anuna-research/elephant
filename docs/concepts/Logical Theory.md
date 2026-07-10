# Logical Theory

elephant's unit of shared context (the prior design called it a
*partition*): an independent, append-only [[Corpus]] of signed
[[Speech Act]]s plus the [[Defeasible Logic]] closure each member derives
from it locally.

Properties:

- **Disjoint.** No literal, rule, member, or trust weight crosses
  theories ([[SPEC-001-elephant-core#REQ-018]]). One agent participates
  in many theories under one [[DID]].
- **Convergent corpus, divergent conclusions — by design.** Replicas
  converge on the same Entry set (CRDT merge); closures may differ only
  through each agent's local trust policy.
- **Identified by history.** The theory id is the blake3 hash of its
  genesis Entry; membership is closure-derived from signed `member`
  facts ([[SPEC-002-elephant-p2p#REQ-105]]).
- **Joined by ceremony.** [[Theory Join]] via invite code + [[SPAKE2]].
