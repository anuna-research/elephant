# User: Autonomous Agent

Role: A non-human participant — an LLM coding agent, a CI pipeline step, a
webhook bridge, a cron job — that reads from and writes to shared
[[Logical Theory]]s programmatically.

Goals:

- Assert signed [[Speech Act]]s (facts, promises, completions, defeaters) as
  side-effects of its own work, attributably and irrevocably.
- Discover what it should do next by querying the theory (ready tasks,
  abduced missing facts), not by being orchestrated.
- React when a watched conclusion changes tag (e.g. `release-ready` flips to
  `+d`) with low latency, without polling.
- Participate in several disjoint theories at once under one identity.

Constraints:

- Consumes and emits machine-readable output only: stable JSON on stdout,
  meaningful exit codes; no TTY affordances, no prompts, no pagers.
- Runs headless and unattended; every operation must be non-interactive once
  identity and theory membership exist (join may be interactive for the
  human who authorises it, never for the agent afterwards).
- May be short-lived (a CI step): needs the long-lived [[Daemon]] to hold
  replicas and connections so a one-shot CLI call is cheap.
- Cannot keep secrets beyond a key file on disk with 0600 permissions.

Daily workflow:

1. Wake (cron/CI trigger/watch event).
2. `elephant status --json` / `elephant next --json` — anything for me?
3. Do the work.
4. `elephant assert '<spl>'` — record the outcome as a signed claim.
5. Exit; the daemon keeps syncing.
