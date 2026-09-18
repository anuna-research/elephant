---
title: Your first theory and promise
mode: tutorial
---

# Your first theory and promise

We will create a [[Logical Theory]], promise an approval, and watch evidence fulfil the [[Commitment]].
This lesson uses a new identity and the local theory alias `release`.
Install Elephant using [[README#Quick Start]] before starting.

Create the signing identity:

```sh
elephant id create --name alice
```

The output identifies Alice's [[DID]]. Subsequent assertions use that identity.
Create the theory:

```sh
elephant theory create release
```

Assert an approval and a rule written in [[SPL]]:

```sh
elephant assert 'qa-signed' -t release
elephant assert '(normally r-release (and qa-signed legal-signed) release-ready)' -t release
```

Ask what prevents readiness:

```sh
elephant why-not release-ready -t release
```

The explanation identifies `legal-signed` as missing support.
Promise that approval before asserting it:

```sh
elephant promise legal-signed -t release
elephant commitments -t release
```

The commitment is `outstanding` because the approval is absent.
Record the approval:

```sh
elephant assert 'legal-signed' -t release
elephant commitments -t release
elephant explain release-ready -t release
```

The commitment becomes `fulfilled`, and the rule derives `release-ready`.
No status update was necessary: the new evidence changed the conclusions.

This example omits a deadline so its result does not depend on the calendar.
Real promises accept a deadline through `--by <RFC3339>`.
[[how-to-coordinate-work]] applies this pattern to tasks and acceptance evidence.
