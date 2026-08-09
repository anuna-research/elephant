#!/usr/bin/env bash
# SPEC-006 `elephant next` — CLI-level acceptance.
#
# TEST-601/602/603/604/609/610/611 are unit tests in src/core/next.rs. The
# checks here need a real binary and a real store, so they cannot be:
#   TEST-605  the emitted token arrays run verbatim and resolve
#   TEST-606  read-only scope invariant (REQ-602)
#   TEST-607  plan paths and the retired task surface are rejected
#   TEST-608  idle is a successful observation (REQ-606)
#   NFR-601   repeated runs are byte-identical
#   OBS-601   the trace event carries counts, and no prose
#
# Usage: scripts/spec006-acceptance.sh [path-to-elephant]
set -uo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-target/debug/elephant}"
[ -x "$BIN" ] || { echo "no elephant binary at $BIN — run 'cargo build' first"; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export ELEPHANT_HOME="$WORK/home"
mkdir -p "$ELEPHANT_HOME"

fail=0
chk() { if [ "$1" = 0 ]; then echo "  PASS $2"; else echo "  FAIL $2"; fail=1; fi }
# Strip the tracing crate's ANSI styling before matching structured fields.
plain() { sed $'s/\033\[[0-9;]*m//g'; }
sum() { find "$ELEPHANT_HOME" -type f | sort | xargs shasum -a 256 2>/dev/null | shasum -a 256; }
e() { "$BIN" "$@" 2>&1 | grep -v '^warning'; }

e id create >/dev/null
e theory create fixture >/dev/null

echo "=== TEST-608: idle is a successful observation ==="
out="$("$BIN" -t fixture next --json 2>/dev/null)"; rc=$?
[ "$rc" = 0 ]; chk $? "exit zero on a theory with no tasks"
echo "$out" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["v"]==1 and d["next"]==[] and d["withheld"]==[], d'
chk $? 'v:1 with next: [] and withheld: []'
echo "$out" | grep -q '"error"'; [ $? != 0 ]; chk $? "no error object emitted"

echo
echo "=== fixture: one fully described ready task ==="
e -t fixture assert '(given (task models))' >/dev/null
e -t fixture assert '(given (task-description models "Design the data model"))' >/dev/null
e -t fixture assert '(given (task-acceptance models "TEST-601 fixture acceptance passes"))' >/dev/null
e -t fixture assert '(given (prerequisite-met models))' >/dev/null
e -t fixture assert '(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))' >/dev/null
e -t fixture assert '(meta r-ready (source "SPEC-006-elephant-next#TEST-601"))' >/dev/null

echo
echo "=== TEST-606: read-only scope invariant (REQ-602) ==="
before_log="$(e -t fixture log --json | shasum -a 256)"
before_home="$(sum)"
e -t fixture next --json >/dev/null
e -t fixture next >/dev/null
[ "$before_log" = "$(e -t fixture log --json | shasum -a 256)" ]; chk $? "corpus entries unchanged"
[ "$before_home" = "$(sum)" ]; chk $? "no local file created, modified or deleted"

echo
echo "=== TEST-607: reject plan paths and the retired task surface (REQ-603) ==="
echo '(given (task x))' > "$WORK/plan.spl"
"$BIN" -t fixture next "$WORK/plan.spl" >/dev/null 2>&1; [ $? != 0 ]; chk $? "next <plan.spl> rejected"
"$BIN" -t fixture task next "$WORK/plan.spl" >/dev/null 2>&1; [ $? != 0 ]; chk $? "task next <plan.spl> rejected"
# Capture first: under `pipefail` the binary's own non-zero exit would mask
# grep's match and make this look like a failure.
reco="$("$BIN" next "$WORK/plan.spl" 2>&1 || true)"
echo "$reco" | grep -qiE 'unexpected argument|error'
chk $? "fails at argument recognition, before theory loading"

echo
echo "=== TEST-605: the emitted token arrays run verbatim (REQ-605) ==="
"$BIN" -t fixture next --json 2>/dev/null | BIN="$BIN" python3 -c '
import json,os,subprocess,sys
d=json.load(sys.stdin)
assert len(d["next"])==1, d
c=d["next"][0]
assert c["ready_literal"]=="(ready models)", c["ready_literal"]
assert c["ready_rule"]=="r-ready", c["ready_rule"]
assert c["source"]=="SPEC-006-elephant-next#TEST-601", c["source"]
assert d["theory"]!="fixture", "alias emitted instead of the resolved id"
for k in ("promise","explain","describe"):
    assert isinstance(c["commands"][k],list) and c["commands"][k][0]=="elephant", k
# the operands themselves, not just argv[0] — this is where canonical
# rendering is observable on the wire
assert c["promise_goal"]=="(completed models)", c["promise_goal"]
assert c["commands"]["promise"][1:3]==["promise","(completed models)"], c["commands"]["promise"]
assert c["commands"]["explain"][1:3]==["explain","(ready models)"], c["commands"]["explain"]
assert c["commands"]["describe"][1:3]==["describe","r-ready"], c["commands"]["describe"]
assert c["commands"]["describe"][-1]=="--json", c["commands"]["describe"]
assert set(c)=={"task","description","acceptance","ready_literal","promise_goal",
                "ready_rule","source","commands"}, sorted(c)
assert set(d)=={"v","theory","next","withheld"}, sorted(d)
for k,needle in (("explain","r-ready"),("describe","SPEC-006-elephant-next#TEST-601")):
    argv=list(c["commands"][k]); argv[0]=os.environ["BIN"]
    r=subprocess.run(argv,capture_output=True,text=True)
    assert r.returncode==0, (k,r.stderr[:300])
    assert needle in r.stdout, (k,needle,r.stdout[:300])
'
chk $? "explain proves via r-ready; describe resolves meta.source"

echo
echo "=== NFR-601: deterministic output ==="
[ "$("$BIN" -t fixture next --json 2>/dev/null)" = "$("$BIN" -t fixture next --json 2>/dev/null)" ]
chk $? "two runs byte-identical"

echo
echo "=== OBS-601: counts without prose ==="
ev="$("$BIN" -v -t fixture next --json 2>&1 >/dev/null | plain | grep 'task discovery projection')"
echo "$ev" | grep -q 'candidates=1'; chk $? "candidate count present"
echo "$ev" | grep -q 'withheld=0'; chk $? "withheld count present"
echo "$ev" | grep -q 'closure=sha256'; chk $? "closure fingerprint present"
echo "$ev" | grep -q 'duration_us='; chk $? "projection duration present"
echo "$ev" | grep -qi 'Design the data model'; [ $? != 0 ]; chk $? "no description text leaked"
echo "$ev" | grep -qi 'fixture acceptance passes'; [ $? != 0 ]; chk $? "no acceptance text leaked"
echo "$ev" | grep -q 'SPEC-006-elephant-next#TEST-601'; [ $? != 0 ]; chk $? "no source text leaked"

echo
if [ "$fail" = 0 ]; then echo "SPEC-006 acceptance: ALL PASS"; else echo "SPEC-006 acceptance: FAILURES"; fi
exit "$fail"
