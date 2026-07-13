# Predicate Family

The unit of vocabulary in a [[Logical Theory]]: the predicate *name*
shared by a set of ground literals, separated from the arguments packed
into them. `ci-green-m1` and `ci-green-m2` are two literals of one
family, `ci-green`; `(blocked-by t "x")` belongs to the family
`blocked-by` (its predicate symbol).

The distinction matters because the frozen hence vocabulary
([[SPEC-003-elephant-tasks]] §1) hyphen-packs arguments into flat atom
names, so the unit worth *documenting and agreeing on* (the family) is
not the unit that appears in conclusions (the ground literal).
Resolution from literal to family is the deterministic suffix-stripping
function of [[SPEC-005-elephant-vocabulary#CON-402]]: parameterised
literals use their predicate symbol; flat atoms strip built-in lifecycle
patterns, then the longest declared-task suffix; anything else is its
own family.

Families are what the vocabulary layer operates on:

- **Documented** by [[SPL]] `meta` on the family atom —
  `(meta ci-green (description …) (kind evidence))`
  ([[SPEC-005-elephant-vocabulary#REQ-402]]).
- **Classified** by role in the admitted theory — a *hole* is a family
  rules listen for but nothing proves; an *orphan* is asserted but
  unconsumed ([[SPEC-005-elephant-vocabulary#REQ-401]]).
- **Built-in** when part of the frozen hence lifecycle/discovery
  vocabulary, which ships pre-documented and immutable
  ([[SPEC-005-elephant-vocabulary#REQ-404]]).
