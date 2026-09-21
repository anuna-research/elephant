---
title: How to install agent guidance
mode: how-to
---

# How to install agent guidance

This guide installs the Elephant skill into a project that already has an Elephant binary.
The skill covers sharing, sync, task discovery, promises, and evidence-derived completion.

## Before you start

Check that the binary includes the installer:

```sh
elephant skill init --help
```

The command is unreleased after `v0.1.7`; older binaries do not include it.
For a source build, follow [How to build and check Elephant](how-to-build-and-check.md).
The installer requires no identity, store, daemon, or network.

## Install

Preview the bundled guidance:

```sh
elephant skill init --dry-run
```

From the project root, install it:

```sh
elephant skill init
```

The default destination is `.agents/skills/elephant/SKILL.md`.
For a different skill directory, specify its path:

```sh
elephant skill init --path .claude/skills/elephant
```

For machine-readable results, add `--json`.
The result contains the path, bundled version, and status; previews also contain the skill text.

## Update and check

Repeat installation to check for identical content.
Identical content remains unchanged; different content causes an error and remains untouched.
After upgrading, review the preview before merging changes into an edited skill.
Alternatively, install into another directory for comparison.

Keep local theory aliases and store paths in project instructions.
The reusable skill does not create identities, join theories, or make promises.
