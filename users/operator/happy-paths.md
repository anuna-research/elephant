# Happy paths — Operator

## HP-O1: First run — identity

Preconditions: elephant installed; no prior state on this machine.
Steps:

1. `elephant id create --name alice` → keypair generated, stored under the
   config dir with 0600 permissions; prints the [[DID]] and name.
2. `elephant id whoami` → same DID, key path, name.

Postconditions: signing identity exists; every later command signs as it.
Failure modes: config dir not writable; identity already exists (refuse to
overwrite; require `--force` semantics decided by spec); clock wildly wrong
(warn — HLC and `:at` stamps depend on it).

## HP-O2: Create a theory and invite a peer

Preconditions: HP-O1 done for Alice on machine A and Bob on machine B.
Steps:

1. Alice: `elephant theory create release-v3` → theory exists locally;
   prints the theory id.
2. Alice: `elephant theory invite release-v3` → prints a short one-time
   invite code (and keeps the process listening, or hands off to the daemon).
3. Alice sends the code to Bob over any channel (Slack, voice, paper).
4. Bob: `elephant theory join <code> --alias release-v3` → [[SPAKE2]]
   exchange completes against Alice's pending invite; Bob's replica syncs the
   corpus; both sides print the other's DID for eyeball verification.

Postconditions: both replicas hold the same corpus; Bob appears as a member;
subsequent assertions from either side reach the other.
Failure modes: code typo (join fails cleanly, no partial state); code reused
(second join refused); Alice offline before Bob joins (join times out with a
clear message; invite can be re-issued); attacker guesses at the code
(single-guess property of PAKE — failed attempt burns the invite).

## HP-O3: Assert and observe

Preconditions: HP-O2; theory `release-v3` shared by Alice and Bob.
Steps:

1. Alice: `elephant assert 'qa-signed' -t release-v3` → the CLI wraps the
   bare literal in `(given qa-signed)`, wraps that in the [[cbcl-elephant]]
   dialect envelope, signs, appends to the local replica; prints a receipt id.
2. Bob: `elephant assert '(normally r-ready (and qa-signed legal-signed) release-ready)' -t release-v3`.
3. Alice: `elephant status -t release-v3` → `qa-signed +d`,
   `legal-signed -d`, `release-ready -d`.
4. Bob: `elephant why-not release-ready -t release-v3` → rule `r-ready`
   found; `qa-signed` held; `legal-signed` missing.
5. Bob: `elephant assert 'legal-signed' -t release-v3`.
6. Alice: `elephant status -t release-v3` → `release-ready +d`.

Postconditions: same conclusions on both machines given same corpus and
default trust.
Failure modes: malformed SPL (parse error before anything is signed or
stored — nothing enters the corpus); daemon not running and peer offline
(assertion lands locally, syncs when connectivity returns — surfaced, not an
error).

## HP-O4: Promise and fulfilment (Elephant 2000 core)

Preconditions: HP-O2.
Steps:

1. Bob: `elephant promise 'legal-signed' --by 2026-07-18T17:00:00Z -t release-v3`
   → a signed promise ([[Commitment]] speech act) enters the corpus; `status` now lists an
   outstanding commitment of Bob's.
2. Alice: `elephant commitments -t release-v3` → Bob's promise, deadline,
   state `outstanding`.
3. Bob (before the deadline): `elephant assert 'legal-signed' -t release-v3`.
4. Either: `elephant commitments -t release-v3` → Bob's promise `fulfilled`.

Postconditions: the corpus contains permanent evidence that Bob promised and
that Bob fulfilled; had the deadline passed first, the commitment would derive
`violated` — visibly, attributably, without any human adjudication.
Failure modes: promise with a past deadline (rejected at parse/validate);
clock skew between peers (deadline evaluation uses the reasoner's time
coordinate; document the tolerance).

## HP-O5: Daemon lifecycle

Preconditions: HP-O2.
Steps:

1. `elephant daemon start` → background process starts, opens replicas for
   all joined theories, begins discovery/sync; `daemon status` shows PID,
   theories, peer counts.
2. Operator closes the laptop lid; reopens an hour later.
3. `elephant status -t release-v3` → conclusions reflect assertions peers
   made in the interim; no manual sync command was ever typed.
4. `elephant daemon stop` → clean shutdown; replicas flushed.

Postconditions: sync is continuous while the daemon runs; one-shot CLI calls
work against the daemon when present and degrade to direct (open store,
no p2p) when absent.
Failure modes: daemon already running (idempotent start reports it); stale
socket/pidfile after crash (start detects and recovers); two daemons racing
on one store (lock; second one refuses).

## HP-O6: Many disjoint theories, one identity

Preconditions: HP-O1.
Steps:

1. Alice joins `release-v3` (work), creates `homelab` (personal), joins
   `conf-2026-pc` (a programme committee).
2. `elephant theory list` → three theories, member counts, sync state.
3. `elephant status -t homelab` → conclusions of that theory only; nothing
   from `release-v3` leaks in — no shared literals, no shared peers, no
   shared trust.
4. Default theory: `-t` omitted uses the configured default or errors with
   the list of candidates (never guesses silently when ambiguous).

Postconditions: theories are fully disjoint corpora with disjoint peer sets;
one DID participates in all three.
Failure modes: alias collision on join (local aliases are per-machine;
collision prompts for a different alias); watching literals of the same name
in two theories (notifications name the theory).
