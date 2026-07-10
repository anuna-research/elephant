# Elephant 2000

John McCarthy, *Elephant 2000: A Programming Language Based on Speech Acts*
(Stanford, 1989–94 draft). The intent-anchor of this project. Its theses, as
elephant-3000 operationalises them:

1. **I/O is speech acts.** Communication consists of meaningful
   [[Speech Act]]s — questions, answers, offers, acceptances, requests,
   promises — not opaque strings. → elephant's wire is the
   [[cbcl-elephant]] dialect whose performatives *are* speech acts.
2. **Correctness = proper performance.** "Answers should be truthful and
   responsive, and promises should be kept." Correctness sentences can be
   generated from the program itself. → a [[Commitment]]'s fulfilment or
   violation is *derived* by the defeasible reasoner from the corpus, not
   adjudicated by anyone.
3. **No data structures — refer directly to the past.** "A passenger has a
   reservation if he has made one and hasn't cancelled it." An Elephant
   interpreter keeps a history list ("journal") of all events. → the
   [[Corpus]] is an append-only CRDT log of signed speech acts; every
   predicate (membership, commitment existence, task state) is a function
   of that history. Nothing is ever deleted — *an elephant never forgets*.
4. **Commitments are first-class abstract objects** with `make`, `cancel`
   (`revoke`) and `exists`, where
   `exists(t, c) ≡ ∃t′<t. arises(t′,c) ∧ ∀t″∈(t′,t). ¬revoke(t″,c)`.
   → `elephant promise` / `elephant retract` / derived `commitment-*`
   literals implement exactly this axiom.
5. **Nonmonotonic closure for verification.** McCarthy circumscribes
   `arises`, `outputs`, `revoke`; the interpreter just fires rules, the
   nonmonotonic closure is used to prove the program keeps its promises. →
   spindle-rust's [[Defeasible Logic]] closure plays the circumscription
   role over the shared theory.
6. **Intrinsic vs accomplishment specifications.** Illocutionary
   correctness (did the program state/promise correctly, given its inputs)
   is checkable from the corpus alone; perlocutionary correctness (did the
   work actually happen in the world) needs world axioms. Elephant checks
   the former mechanically and records evidence for the latter as trusted
   claims.

Epigraph, which the tool honours literally: "I meant what I said, and I
said what I meant… an elephant never forgets."
