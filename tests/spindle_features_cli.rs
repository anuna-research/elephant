//! Spindle feature integration through signed entries and the public CLI.
use assert_cmd::cargo::cargo_bin;
use serde_json::{Value, json};
use std::process::Command;

struct Env {
    dir: tempfile::TempDir,
}
impl Env {
    fn new() -> Self {
        let e = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        e.json(&["id", "create", "--name", "features"]);
        e.json(&["theory", "create", "features"]);
        e
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(cargo_bin("elephant"));
        c.env("ELEPHANT_HOME", self.dir.path())
            .args(["--json", "-t", "features"]);
        c
    }
    fn json(&self, args: &[&str]) -> Value {
        let out = self.cmd().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn assert(&self, spl: &str) -> Value {
        self.json(&["assert", spl])
    }
    fn set_extensions(&self, value: &Value) -> Value {
        let path = self.dir.path().join("functions.json");
        std::fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
        self.json(&["extensions", "set", path.to_str().unwrap()])
    }
    fn fails(&self, args: &[&str], message: &str) {
        let out = self.cmd().args(args).output().unwrap();
        assert!(!out.status.success(), "unexpected success: {args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(message),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.cmd().args(["daemon", "stop"]).output();
    }
}

fn extensions(label: &str) -> Value {
    json!({"schema_version":"spindle.extensions.v1", "functions":[{
        "name":"classification", "arguments":["integer"], "returns":"symbol",
        "rows":[
            {"args":[{"type":"integer","value":2}], "result":{"type":"symbol","value":label}},
            {"args":[{"type":"integer","value":5}], "result":{"type":"symbol","value":"extra"}}
        ]
    }], "aggregators":[{"name":"total", "reducer":"+", "identity":0}]})
}

fn positive(output: &Value, functor: &str, value: Value) -> bool {
    output["conclusions"].as_array().unwrap().iter().any(|c| {
        c["positive"] == true
            && c["literal_struct"]["functor"] == functor
            && c["literal_struct"]["args"].as_array().unwrap().last() == Some(&value)
    })
}

#[test]
fn aggregates_signed_provenance_and_hypothetical_recomputation() {
    let e = Env::new();
    e.assert(
        "(given (purchase a 10)) (given (purchase b 10))
        (normally paid (purchase ?id ?cost) (payment ?id ?cost))
        (normally total (agg ?n sum ?cost (payment ?id ?cost)) (total ?n))",
    );
    let output = e.json(&["reason", "--v2"]);
    assert_eq!(output["schema_version"], "spindle.reason.v2");
    assert!(
        positive(&output, "total", json!({"type":"integer","value":20})),
        "{output}"
    );
    assert!(
        !output["conclusions"]
            .to_string()
            .contains("__aggregate_snapshot")
    );
    assert!(!e.json(&["explain", "(total 20)"])["explanation"].is_null());
    let audit = e.json(&["describe", "paid"]);
    assert!(
        audit.to_string().contains("agent:"),
        "provenance retained: {audit}"
    );
    let hypothetical = e.json(&["what-if", "(purchase c 5)", "(total 25)"]);
    assert_eq!(hypothetical["provable"], true, "{hypothetical}");
    assert!(positive(
        &e.json(&["reason", "--v2"]),
        "total",
        json!({"type":"integer","value":20})
    ));
    e.fails(&["status", "--trust"], "trust-weighted aggregation");
}

#[test]
fn shared_functions_replacement_retraction_and_query_options() {
    let e = Env::new();
    let initial = extensions("small");
    e.set_extensions(&initial);
    assert_eq!(e.json(&["extensions", "show"]), initial);
    e.assert(
        "(given (size 2))
        (normally classify (and (size ?n) (bind ?label (classification ?n))) (class ?label))
        (normally ready (and (class small) approved) ready)",
    );
    assert!(positive(
        &e.json(&["reason", "--v2"]),
        "class",
        json!({"type":"symbol","value":"small"})
    ));
    assert!(!e.json(&["explain", "class(small)"])["explanation"].is_null());
    assert_eq!(e.json(&["what-if", "approved", "ready"])["provable"], true);
    assert_eq!(
        e.json(&["what-if", "(size 5)", "(class extra)"])["provable"],
        true,
        "hypotheses must be inserted before grounding with the same registry"
    );
    let requires = e.json(&["require", "ready"]);
    assert!(
        requires["solutions"]
            .as_array()
            .unwrap()
            .contains(&json!(["approved"])),
        "{requires}"
    );
    assert_eq!(requires["verification_mode"], "verified");
    let raw = e.json(&["abduce", "ready"]);
    assert_eq!(raw["verification_mode"], "raw");
    assert!(raw["solutions"][0]["facts_struct"].is_array());
    let second = e.set_extensions(&extensions("large"));
    assert!(positive(
        &e.json(&["reason", "--v2"]),
        "class",
        json!({"type":"symbol","value":"large"})
    ));
    e.json(&["retract", second["receipt"].as_str().unwrap()]);
    assert_eq!(e.json(&["extensions", "show"]), initial);
    assert!(positive(
        &e.json(&["reason", "--v2"]),
        "class",
        json!({"type":"symbol","value":"small"})
    ));
}

#[test]
fn custom_aggregators_and_lookup_bindings_in_aggregate_program() {
    let e = Env::new();
    e.set_extensions(&extensions("small"));
    e.assert(
        "(given (size 2))
        (normally classify (and (size ?n) (bind ?label (classification ?n))) (class ?label))
        (normally total (agg ?n total ?size (size ?size)) (total ?n))",
    );
    let out = e.json(&["reason", "--v2"]);
    assert!(
        positive(&out, "class", json!({"type":"symbol","value":"small"})),
        "{out}"
    );
    assert!(
        positive(&out, "total", json!({"type":"integer","value":2})),
        "{out}"
    );
}

#[test]
fn literal_types_vocabulary_and_strict_query_input() {
    let e = Env::new();
    e.set_extensions(&extensions("2"));
    e.assert(
        "(predicate value ((n any))) (given (value 2)) (given (size 2))
        (normally symbol-value (and (size ?n) (bind ?label (classification ?n))) (value ?label))",
    );
    let output = e.json(&["reason", "--v2"]);
    let v1 = e.json(&["reason"]);
    let schema: Value = serde_json::from_str(include_str!(
        "../../spindle-rust/contracts/spindle/v1/schemas/spindle.reason.v1.schema.json"
    ))
    .unwrap();
    for key in v1.as_object().unwrap().keys() {
        assert!(
            schema["properties"].get(key).is_some(),
            "non-contract field: {key}"
        );
    }
    for key in schema["required"].as_array().unwrap() {
        assert!(
            v1.get(key.as_str().unwrap()).is_some(),
            "missing contract field: {key}"
        );
    }
    assert!(
        positive(&output, "value", json!({"type":"integer","value":2})),
        "{output}"
    );
    assert!(
        positive(&output, "value", json!({"type":"symbol","value":"2"})),
        "{output}"
    );
    assert_eq!(
        output["conclusions"],
        e.json(&["reason", "--v2"])["conclusions"]
    );
    let vocabulary = e.json(&["vocab", "--spindle"]);
    assert_eq!(vocabulary["schema"], "spindle.vocabulary/1");
    for bad in [
        "value) (given injected",
        "(value ?x)",
        "(value 2) (value 3)",
    ] {
        e.fails(&["explain", bad], "ground literal");
    }
    assert!(!e.json(&["explain", "value 2"])["explanation"].is_null());
}

#[test]
fn reference_time_is_shared_by_follow_up_queries() {
    let e = Env::new();
    e.assert("(given (during evidence 100 200)) (normally ready evidence ready)");
    let early = ["--at", "1970-01-01T00:00:00.150Z"];
    let late = ["--at", "1970-01-01T00:00:00.300Z"];
    assert!(!e.json(&["explain", "evidence", early[0], early[1]])["explanation"].is_null());
    assert!(e.json(&["explain", "evidence", late[0], late[1]])["explanation"].is_null());
    assert_eq!(
        e.json(&["what-if", "unrelated", "evidence", late[0], late[1]])["provable"],
        false
    );
    assert_eq!(
        e.json(&["require", "evidence", late[0], late[1]])["already_provable"],
        false
    );
}

#[test]
fn invalid_extensions_are_refused_before_append() {
    let e = Env::new();
    let before = e.json(&["log"]);
    let invalid = e.dir.path().join("invalid.json");
    std::fs::write(&invalid, r#"{"schema_version":"wrong"}"#).unwrap();
    e.fails(
        &["extensions", "set", invalid.to_str().unwrap()],
        "spindle.extensions.v1",
    );
    assert_eq!(e.json(&["log"]), before);
    // Escaped JSON string values survive both the SPL and CBCL layers.
    let document = extensions("a\"b\\c;d\nnext");
    e.set_extensions(&document);
    assert_eq!(e.json(&["extensions", "show"]), document);
}

#[test]
fn daemon_uses_shared_extensions_and_aggregation() {
    let e = Env::new();
    e.json(&["daemon", "start"]);
    e.set_extensions(&extensions("small"));
    e.assert("(given (size 2)) (normally total (agg ?n total ?size (size ?size)) (total ?n))");
    let output = e.json(&["reason", "--v2"]);
    assert!(
        positive(&output, "total", json!({"type":"integer","value":2})),
        "{output}"
    );
    assert!(
        e.json(&["daemon", "status"])["counters"]["closures_run"]
            .as_u64()
            .unwrap()
            >= 2
    );
}

#[test]
fn grounded_trust_details_include_diminishers_sources_and_thresholds() {
    let e = Env::new();
    let receipt = e.assert(
        "(given (evidence task)) (given (concern task))
        (normally support (evidence ?x) (approved ?x))
        (except challenge (concern ?x) (not (approved ?x)))
        (prefer support challenge)",
    );
    let source = elephant::core::closure::source_atom(receipt["signer"].as_str().unwrap());
    let policy_dir = e.dir.path().join("trust");
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(
        policy_dir.join(format!("{}.spl", receipt["theory"].as_str().unwrap())),
        format!("(trusts {source} 0.8) (threshold action 0.5)"),
    )
    .unwrap();
    let output = e.json(&["status", "--trust"]);
    let row = output["conclusions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["literal"] == "approved task")
        .unwrap();
    assert!(
        (row["degree"].as_f64().unwrap() - 0.16).abs() < 1e-9,
        "{row}"
    );
    assert_eq!(row["above_threshold"]["action"], false);
    assert_eq!(row["sources"], json!([source]));
    assert!(
        !row["trust_details"]["diminished_by"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let full = e.json(&["reason", "--v2", "--trust"]);
    let degree = full["conclusions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["literal_spl"] == "(approved task)" && r["conclusion_type"] == "+d")
        .unwrap();
    assert_eq!(degree["trust_details"], row["trust_details"]);
    assert_eq!(
        e.json(&["what-if", "unrelated", "(approved task)"])["provable"],
        true
    );
    assert_eq!(
        e.json(&["require", "(approved task)"])["already_provable"],
        true
    );
}

#[test]
fn aggregate_limits_are_explicit_errors() {
    for extra in ["(given (during timed 1 2))", "(trusts someone 0.8)"] {
        let e = Env::new();
        e.assert(&format!(
            "(given (size 2)) (normally r-total (agg ?n sum ?x (size ?x)) (total ?n)) {extra}"
        ));
        e.fails(
            &["reason", "--v2"],
            if extra.contains("during") {
                "temporal"
            } else {
                "trust-weighted"
            },
        );
    }
}

#[test]
fn library_host_registry_survives_hypothetical_regrounding() {
    use spindle_core::{
        Arity, EvalError, ExtensionFunction, FunctionRegistry, FunctionSignature, Term,
    };
    struct Double(FunctionSignature);
    impl ExtensionFunction for Double {
        fn signature(&self) -> &FunctionSignature {
            &self.0
        }
        fn eval(&self, args: &[Term]) -> Result<Term, EvalError> {
            match args {
                [Term::Integer(n)] => n
                    .checked_mul(2)
                    .map(Term::Integer)
                    .ok_or_else(|| EvalError::EvalFailed("overflow".into())),
                _ => Err(EvalError::TypeError("expected one integer".into())),
            }
        }
    }
    let e = Env::new();
    e.assert(
        "(given (input 2)) (normally doubled (and (input ?n) (bind ?v (double ?n))) (output ?v))",
    );
    let paths = elephant::paths::Paths {
        home: e.dir.path().to_owned(),
    };
    let store = elephant::store::TheoryStore::open(&paths, "features").unwrap();
    let resolve = store.key_resolver();
    let mut registry = FunctionRegistry::new();
    registry.register(Box::new(Double(FunctionSignature {
        name: spindle_core::intern("double"),
        arity: Arity::Fixed(1),
        description: "integer doubling",
    })));
    let closure = elephant::core::closure::close_with_options(
        &store.entries().0,
        &store.theory_id,
        elephant::store::GENESIS_THEORY,
        &resolve,
        "",
        150,
        spindle_core::pipeline::PrepareOptions {
            function_registry: Some(registry),
            ..Default::default()
        },
    )
    .unwrap();
    let result = elephant::core::reasoning::what_if(
        &closure,
        vec![spindle_core::query::HypotheticalClaim::new(
            elephant::queries::parse_literal("(input 3)").unwrap(),
        )],
        &elephant::queries::parse_literal("(output 6)").unwrap(),
    )
    .unwrap();
    assert!(result.is_provable());
    assert!(
        !closure
            .conclusions
            .iter()
            .any(|c| c.is_positive() && c.literal.to_spl() == "(output 6)")
    );
}
