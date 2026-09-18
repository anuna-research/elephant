---
title: How to share a theory
mode: how-to
---

# How to share a theory

This guide shares an existing [[Logical Theory]] between independently attributed agents.
The steward owns the existing theory; the joining agent uses a separate [[DID]] and store.

## Prepare the agents

Inspect each agent's identity and local theories:

```sh
elephant info --json
```

On one machine, assign each agent a distinct absolute `ELEPHANT_HOME` directory.
Keep that setting for the agent's commands and daemon.
Sharing a store shares its signer; copying identity keys does not create an independent peer.

If the recipient has no identity in its selected store, create only its identity:

```sh
elephant id create --name reviewer
```

Reserve an unused local alias for the join.
The commands below assume `release` is unused on the recipient's side.
If that alias already names another theory, choose a different alias for `--alias` and subsequent recipient commands.

Use the existing theory on the steward's side.
Creating another theory with the same alias creates a different theory.
Aliases are local; the full theory ID identifies the shared corpus.

## Invite and join

On the steward's side, start an invitation:

```sh
elephant theory invite release --ttl 15m
```

Keep the process running while the recipient joins.
Deliver the one-time code through the agreed out-of-band channel.
Do not put it in committed files, theory assertions, or conversation logs.
Joining grants access to the theory's history and participation in it.

On the recipient's side, start the join and enter the code at its prompt:

```sh
elephant theory join --alias release
```

If an invitation expires or fails, inspect the error before issuing a fresh invitation.
The steward alone can invite members.

## Enable ongoing sync

After joining, run these commands on both sides:

```sh
elephant theory list --json
elephant theory members release --json
ELEPHANT_SYNC_INTERVAL=30 elephant daemon start
elephant daemon status --json
```

Check that the theory IDs match and that the roster contains both expected DIDs.
Check the reported sync interval and peer health.
Joining transfers history; ongoing updates require a running [[Daemon]] with sync enabled.
An unset or zero `ELEPHANT_SYNC_INTERVAL` disables continuous sync.

If a daemon already runs, change its launch environment and restart it to apply a new sync interval.
For a manually started daemon, use:

```sh
elephant daemon stop
ELEPHANT_SYNC_INTERVAL=30 elephant daemon start
```

For launchd services, use [[how-to-run-elephant-on-login]].
Starting an already running daemon returns its existing instance; it does not apply the caller's new environment.

## Check the shared result

Inspect the expected peer assertion with `log` or `show`:

```sh
elephant log -t release --status active --performative assert
elephant status -t release --json
```

For a semantic comparison, inspect the fingerprints described in [[inspection]].
When comparing time-sensitive conclusions, use the same explicit `--at` time.
Matching fingerprints do not establish identical journals or membership.

To wait for a peer's conclusion, watch the literal:

```sh
elephant watch release-ready -t release
```

The watch remains active until interrupted.
If updates stop, inspect daemon status and peer errors before treating missing evidence as failed work.
