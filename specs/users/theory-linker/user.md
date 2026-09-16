---
title: Theory linker and report publisher
mode: reference
---

# User: Theory linker and report publisher

Role: operator or automation connecting separate project, research, QA, and release groups.

Goals:
- Share a stable reference that another machine can interpret.
- Follow evidence already accessible locally.
- Disclose a selected result without publishing its private premises.
- Identify who reported a result and when its validity ends.
- Decide explicitly whether that report supports a local action.

Constraints: terminal or stable JSON; intermittent connectivity; different group memberships; no expectation of global backlink completeness.
The publisher has access to the source and destination.
The recipient can lack source access entirely.
Automation needs explicit parameters rather than interactive prompts.

Daily workflow: inspect source → select evidence → publish deliberately → inspect recipient evidence → author or evaluate bridge rules → withdraw obsolete reports.
Source: the stakeholder conversation about theory links and the QA-to-release use-case, 2026-09-16.
This profile is a design hypothesis awaiting human validation, not measured user research.
Paths: [[users/theory-linker/happy-paths]].
