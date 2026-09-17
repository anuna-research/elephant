# Spindle reasoning in Elephant

Elephant exposes Spindle's arithmetic, integer/symbol aggregation, portable
extension functions, predicate vocabulary, temporal reasoning, and trust
diminishment through signed theory entries. Run `elephant capabilities --json`
for the interface and supported-fragment inventory.

Source builds require the Spindle interface-parity revision
`ede0ec27bb35abff1b8f9b4cb2af6747bd796049` (or a compatible descendant),
available on `deps/elephant-spindle-v040`. CI and release workflows pin that
revision and the other sibling dependencies to the checkouts used for testing.

## Reasoning and queries

```sh
elephant -t demo status --json
elephant -t demo reason --v2 --json
elephant -t demo reason --v2 --trust --json
elephant -t demo explain '(approved task)' --json
elephant -t demo why-not '(approved task)' --json
elephant -t demo require '(approved task)' --max-solutions 8 --max-raw-candidates 1000 --json
elephant -t demo abduce '(approved task)' --max-solutions 8 --json
elephant -t demo what-if '(evidence task)' '(approved task)' --json
elephant -t demo vocab --spindle --json
```

`status` retains its effective-tag view. `reason --json` returns the full
`spindle.reason.v1` contract; `--v2` selects `spindle.reason.v2`, preserving
symbol, integer, decimal, and float argument types. All four proof tags remain
visible. The structured literals also retain modality, negation, and temporal
bounds. Existing Elephant JSON views gain additive typed fields alongside their
display strings. The legacy `elephant.closure.v1` fingerprint still hashes
display strings; use typed reasoning output when comparing argument types.

`status --trust` and `reason --trust` expose source identities, diminished
degrees, named threshold results, and the defeaters responsible for
diminishment. Trust is calculated against the prepared, grounded rules, with
claim timestamps evaluated at the selected reference time.

Queries accept ground SPL literals, legacy `p(a, b)` notation, and Elephant's
flat `p a b` spelling. Variables, multiple literals, and trailing forms are
rejected. The SPL parser determines argument types; quoting a numeral in SPL
does not force a symbol. Typed lookup results can produce numeric-looking
symbols without coercion.

`require` verifies candidate fact sets by inserting them into the original
source and preparing it again. `abduce` exposes **unverified** candidates and
marks them `verification_mode: raw`. Both are bounded searches, not complete
inverse solvers for arithmetic or aggregation. Hypothetical facts enter before
grounding so new bindings and changed aggregate totals are considered.
Explanations, why-not, requirements, and what-if use the closure's reference
time and extension registry. `--at` is a read-time lens, not a journal cutoff.

## Aggregation

```sh
elephant -t demo assert '(given (purchase a 10)) (given (purchase b 10))'
elephant -t demo assert '(normally total-cost (agg ?n sum ?cost (purchase ?id ?cost)) (total ?n))'
elephant -t demo reason --v2 --json
elephant -t demo what-if '(purchase c 5)' '(total 25)' --json
```

`sum`, `count`, `min-of`, `max-of`, explicit `bind`/`fold`, and registered named
aggregators use Spindle's completed-snapshot semantics. Signed assertion
provenance remains available through `describe` and the journal. Elephant
omits source annotations only from the unweighted aggregate input passed to
Spindle; the original theory and audit metadata are retained. Internal snapshot
predicates are not exposed as conclusions or suggested as abductive remedies.

Spindle's aggregate fragment currently excludes temporal or modal programs,
decimal/float inputs, and trust-weighted snapshots. Those combinations fail
explicitly. `--trust` also fails on an aggregate theory instead of inventing
degrees. An atemporal aggregate theory can be evaluated with `--at`; there is
no temporal filtering to perform in that fragment. Cycles, overflow, and
grounding-budget exhaustion remain engine errors.

## Shared lookup functions and aggregators

Save this as `lookups.json`:

```json
{
  "schema_version": "spindle.extensions.v1",
  "functions": [{
    "name": "classification",
    "arguments": ["integer"],
    "returns": "symbol",
    "rows": [{
      "args": [{"type": "integer", "value": 2}],
      "result": {"type": "symbol", "value": "small"}
    }]
  }],
  "aggregators": [{"name": "total", "reducer": "+", "identity": 0}]
}
```

```sh
elephant -t demo extensions set lookups.json
elephant -t demo extensions show --json
elephant -t demo assert '(given (size 2))'
elephant -t demo assert '(normally classify (and (size ?n) (bind ?label (classification ?n))) (class ?label))'
elephant -t demo explain '(class small)' --json
```

`extensions set` validates the entire document before signing an assertion:
`(meta elephant-extensions (document "…JSON…"))`. Definitions travel with the
theory through the existing encrypted sync channel. They are finite, typed
lookup tables and named reductions; no executable plugin or network callback
is installed. Builtin names, duplicate definitions/keys, and invalid signatures
are rejected according to Spindle's portable extension contract.

The last active document in corpus order replaces the registry in full. Any
member who can assert may publish a document, just as they can publish rules.
Use the returned receipt with `retract` to withdraw your document; the preceding
active document becomes effective again. Replacing/removing definitions still
referenced by rules may make those rules fail preparation. The CLI, daemon,
membership closure, and query paths all resolve definitions from the same
active signed entries. `--at` does not choose an older extension document.

Rust embedders can use `core::closure::close_with_options` with Spindle's
`PrepareOptions`, including a host `FunctionRegistry` and grounding budgets.
Shared definitions augment that registry and win name collisions. Host
functions must be pure and deterministic; callers are responsible for making
the same host functions available to every participant. CLI registries need no
host code and use only the portable shared document.
