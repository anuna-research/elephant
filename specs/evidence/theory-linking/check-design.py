#!/usr/bin/env python3
"""Check draft trace links and the report eligibility design model, not product code."""
import itertools
import json
import re
from pathlib import Path

VAULT = Path(__file__).resolve().parents[2]
NAMES = ("SPEC-007-theory-references", "SPEC-008-theory-reports")
errors = []
pages = {p.stem: p for p in VAULT.rglob("*.md")}


def resolve(target):
    candidate = VAULT / (target + ".md")
    return candidate if candidate.is_file() else pages.get(target)


for name in NAMES:
    text = (VAULT / (name + ".md")).read_text()
    sections = re.split(r"(?m)^### ((?:REQ|NFR|CON|ADR|TEST)-\d+) —[^\n]*\n", text)
    nodes = dict(zip(sections[1::2], sections[2::2]))
    ids = sections[1::2]
    if len(ids) != len(set(ids)):
        errors.append(f"{name}: duplicate artefact ID")
    for target in re.findall(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]", text):
        page, _, anchor = target.partition("#")
        p = resolve(page)
        if not p:
            errors.append(f"{name}: unresolved page {page}")
            continue
        if anchor:
            headings = re.findall(r"(?m)^#{1,6} (.+)$", p.read_text())
            if not any(h == anchor or h.startswith(anchor + " ") or h.startswith(anchor + ":") for h in headings):
                errors.append(f"{name}: unresolved anchor {target}")
    for ident, body in nodes.items():
        if ident.startswith(("REQ-", "NFR-")):
            linked = re.findall(r"\[\[" + re.escape(name) + r"#(TEST-\d+)\]\]", body)
            if not linked:
                errors.append(f"{name}#{ident}: no linked test")
            for test in linked:
                if test not in nodes or f"[[{name}#{ident}]]" not in nodes[test]:
                    errors.append(f"{name}#{ident}: missing inverse attribution for {test}")
        if ident.startswith("TEST-"):
            match = re.search(r"Validates: \[\[" + re.escape(name) + r"#((?:REQ|NFR)-\d+)\]\]", body)
            if not match or match[1] not in nodes:
                errors.append(f"{name}#{ident}: missing requirement attribution")
            elif f"[[{name}#{ident}]]" not in nodes[match[1]]:
                errors.append(f"{name}#{ident}: missing requirement-to-test link")


# Independent finite design model: 1000 ms observation, 2000 ms expiry.
# This exercises the contract, not Elephant's unimplemented report projection.
def eligible(t, mode, retracted, ambiguous):
    return mode == "on" and not retracted and not ambiguous and 1000 <= t < 2000


states = list(itertools.product((999, 1000, 1999, 2000, 2001),
                                ("off", "observe", "on"), (False, True), (False, True)))
for t, mode, retracted, ambiguous in states:
    emits = eligible(t, mode, retracted, ambiguous)
    if emits and not (1000 <= t < 2000 and mode == "on" and not retracted and not ambiguous):
        errors.append("temporal model violates eligibility safety")
positive_trace = [(t, eligible(t, "on", False, False)) for t in (999, 1000, 1999, 2000)]
assert positive_trace == [(999, False), (1000, True), (1999, True), (2000, False)]
mutant_emits_at_expiry = 1000 <= 2000 <= 2000
mutant_rejected = mutant_emits_at_expiry and not eligible(2000, "on", False, False)
assert mutant_rejected
result = {
    "scope": "specification links and finite design model only; not implementation verification",
    "result": "fail" if errors else "pass",
    "errors": errors,
    "temporal_exemplification": positive_trace,
    "inclusive_expiry_mutant_rejected": mutant_rejected,
    "product_tests_executed": False,
}
print(json.dumps(result, indent=2))
raise SystemExit(bool(errors))
