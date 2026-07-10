# User: Operator

Role: Software developer or team lead coordinating work across a mixed pool of
actors — humans, LLM agents, CI bots, webhooks — from a terminal.

Goals:

- Stand up a shared [[Logical Theory]] for a piece of work (a release, an
  incident, a contract) in under a minute, without running a server.
- Let teammates and bots join that theory with a single copy-pasteable code,
  without exchanging keys out of band or configuring infrastructure.
- Assert facts, rules, and [[Speech Act]]s ("QA signed off", "I promise to
  review by Friday") and see the consequences immediately.
- Ask *why* a conclusion holds, *why not*, and *what would it take* — and get
  answers derived from signed evidence, not from a status column.
- Walk away, come back later, and find the theory converged: the daemon kept
  syncing while the terminal was closed.

Constraints:

- Comfortable with CLIs (git, docker level); NOT a logician. SPL syntax must be
  learnable from `--help` and examples; common actions must not require writing
  raw SPL.
- Works behind NAT on a laptop; no public IP, no port forwarding, no server to
  administer.
- Intermittently offline (train, flight); must be able to assert locally and
  sync later.
- Security-aware: will not paste private keys into terminals or config shared
  with the team; expects signing to be automatic once identity is set up.

Daily workflow:

1. `elephant status` over morning coffee — what became derivable overnight?
2. Assert completions / claims as work happens.
3. `elephant why-not <goal>` when something is stuck; chase the missing facts.
4. Create a fresh theory when a new work stream starts; archive mentally when
   done (the corpus persists; attention moves on).
