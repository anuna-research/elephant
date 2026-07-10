#!/usr/bin/env bash
# elephant demo — the Elephant 2000 release-gate story, end to end, on one
# machine. Two identities in two state dirs; a promise made, kept, and shown.
set -euo pipefail

ELE="${ELE:-cargo run -q --}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export A="$tmp/alice" B="$tmp/bob"

say() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }

say "Alice creates an identity and a theory"
ELEPHANT_HOME="$A" $ELE id create --name alice
ELEPHANT_HOME="$A" $ELE theory create release

say "Alice asserts a fact and a release-gate rule"
ELEPHANT_HOME="$A" $ELE assert 'qa-signed' -t release
ELEPHANT_HOME="$A" $ELE assert \
  '(normally r-ready (and qa-signed legal-signed docs-ready) release-ready)' -t release

say "status: the gate is not yet open"
ELEPHANT_HOME="$A" $ELE status -t release

say "why-not: what is missing?"
ELEPHANT_HOME="$A" $ELE why-not release-ready -t release

say "require: the minimal facts that would open the gate"
ELEPHANT_HOME="$A" $ELE require release-ready -t release

say "Bob promises to get legal sign-off by a deadline"
ELEPHANT_HOME="$A" $ELE promise 'legal-signed' --by 2036-01-01T00:00:00Z -t release
ELEPHANT_HOME="$A" $ELE commitments -t release

say "The remaining facts land; the gate opens"
ELEPHANT_HOME="$A" $ELE assert 'legal-signed' -t release
ELEPHANT_HOME="$A" $ELE assert 'docs-ready' -t release
ELEPHANT_HOME="$A" $ELE status -t release

say "commitments: the promise is now fulfilled — the elephant remembers"
ELEPHANT_HOME="$A" $ELE commitments -t release

say "explain: the derivation, with provenance"
ELEPHANT_HOME="$A" $ELE explain release-ready -t release

say "log: the full journal, nothing forgotten"
ELEPHANT_HOME="$A" $ELE log -t release
