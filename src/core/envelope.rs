//! Entry — the corpus element — and the cbcl-elephant wire (SPEC-001
//! CON-002/CON-003, REQ-022).
//!
//! An Entry wraps one canonical CBCL speech-act message:
//!   `(signed :signer "<did>" :key "<key-id>" :ts "<rfc3339>"
//!      (with-limits :ttl "<ttl>"
//!        (lang cbcl-elephant (<performative> <args…>))))`
//!
//! Design notes (deliberate divergences, documented):
//! - SPL payloads ride as *strings* inside the CBCL args, not spliced
//!   S-expressions: SPL's lexicon (floats, `agent:alice` atoms) is not a
//!   subset of CBCL's, and LangSec wants exactly one recogniser per
//!   language — SPL text is only ever parsed by `spindle_parser::parse_spl`.
//! - One load-bearing signature: `Entry.sig` (Ed25519 over the
//!   domain-separated signing input below). The CBCL `signed` wrapper
//!   carries signer metadata for R4 structural presence; embedding a
//!   second signature inside the signed bytes would be circular.

use crate::errors::{AppError, AppResult};
use cbcl_core::message::{Message, WrapperType};
use cbcl_core::sexpr::{Atom, SExpr};
use ed25519_dalek::{Signer as _, Verifier as _};
use serde::{Deserialize, Serialize};

pub const ENTRY_VERSION: u16 = 1;
const DS_TAG: &[u8] = b"elephant-entry-v1";
const SID_TAG: &[u8] = b"elephant-sid-v1";

/// Hybrid logical clock stamp; `node_id` is bound to the signer's key
/// (did-crdt `node_id_from_pubkey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc {
    pub wall_ms: u64,
    pub logical: u32,
    pub node_id: u64,
}

impl Hlc {
    pub fn canonical_bytes(&self) -> [u8; 20] {
        let mut out = [0u8; 20];
        out[..8].copy_from_slice(&self.wall_ms.to_be_bytes());
        out[8..12].copy_from_slice(&self.logical.to_be_bytes());
        out[12..].copy_from_slice(&self.node_id.to_be_bytes());
        out
    }
}

/// One corpus element: a signed, canonical CBCL speech-act message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub v: u16,
    pub theory: String,
    pub hlc: Hlc,
    pub signer: String,
    pub key_id: String,
    pub cbcl: String,
    #[serde(with = "sig_hex")]
    pub sig: [u8; 64],
}

impl Entry {
    /// Build and sign an Entry from a speech act.
    pub fn create(
        theory: &str,
        hlc: Hlc,
        signer_did: &str,
        key_id: &str,
        act: &SpeechAct,
        ts_rfc3339: &str,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> Entry {
        let cbcl = build_wire(act, signer_did, key_id, ts_rfc3339);
        let mut e = Entry {
            v: ENTRY_VERSION,
            theory: theory.to_string(),
            hlc,
            signer: signer_did.to_string(),
            key_id: key_id.to_string(),
            cbcl,
            sig: [0u8; 64],
        };
        e.sig = signing_key.sign(&e.signing_input()).to_bytes();
        e
    }

    /// Domain-separated bytes covered by the signature.
    pub fn signing_input(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.theory.len() + self.cbcl.len());
        lp(&mut out, DS_TAG);
        lp(&mut out, self.theory.as_bytes());
        out.extend_from_slice(&self.hlc.canonical_bytes());
        lp(&mut out, self.cbcl.as_bytes());
        out
    }

    /// Receipt / sentence identifier: stable before the bytes exist
    /// (derived from signer + clock, not content) so the id can appear
    /// inside the CBCL text it names.
    pub fn sentence_id(theory: &str, signer: &str, hlc: Hlc) -> String {
        let mut input = Vec::new();
        lp(&mut input, SID_TAG);
        lp(&mut input, theory.as_bytes());
        lp(&mut input, signer.as_bytes());
        input.extend_from_slice(&hlc.canonical_bytes());
        let hash = blake3::hash(&input);
        format!("s-{}", &hash.to_hex().as_str()[..16])
    }

    pub fn verify_sig(&self, pubkey: &ed25519_dalek::VerifyingKey) -> bool {
        pubkey
            .verify(
                &self.signing_input(),
                &ed25519_dalek::Signature::from_bytes(&self.sig),
            )
            .is_ok()
    }
}

fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

// ── speech acts ─────────────────────────────────────────────────────────

/// The seven cbcl-elephant performatives, elephant-typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechAct {
    Assert {
        sentence_id: String,
        spl: String,
    },
    Retract {
        target: String,
        reason: String,
    },
    Query {
        literal: String,
        explain: bool,
    },
    Concede {
        literal: String,
        in_reply_to: String,
    },
    Commit {
        sentence_id: String,
        /// SPL body text; empty string means unconditional (trigger ≡ true).
        trigger: String,
        /// Optional deadline, RFC 3339.
        by: Option<String>,
        /// Goal literal text.
        goal: String,
    },
    Request {
        request_id: String,
        addressee: String,
        trigger: String,
        goal: String,
    },
    Justify {
        conclusion_id: String,
        premises: String,
    },
}

impl SpeechAct {
    pub fn performative(&self) -> &'static str {
        match self {
            SpeechAct::Assert { .. } => "assert",
            SpeechAct::Retract { .. } => "retract",
            SpeechAct::Query { .. } => "query",
            SpeechAct::Concede { .. } => "concede",
            SpeechAct::Commit { .. } => "commit",
            SpeechAct::Request { .. } => "request",
            SpeechAct::Justify { .. } => "justify",
        }
    }
}

fn sym(s: &str) -> SExpr {
    SExpr::Atom(Atom::Symbol(s.to_string()))
}
fn kw(s: &str) -> SExpr {
    SExpr::Atom(Atom::Keyword(s.to_string()))
}
fn string(s: &str) -> SExpr {
    SExpr::Atom(Atom::Str(s.to_string()))
}

fn act_sexpr(act: &SpeechAct) -> SExpr {
    let items = match act {
        SpeechAct::Assert { sentence_id, spl } => {
            vec![sym("assert"), string(sentence_id), string(spl)]
        }
        SpeechAct::Retract { target, reason } => {
            vec![sym("retract"), string(target), string(reason)]
        }
        SpeechAct::Query { literal, explain } => vec![
            sym("query"),
            string(literal),
            SExpr::Atom(Atom::Bool(*explain)),
        ],
        SpeechAct::Concede {
            literal,
            in_reply_to,
        } => vec![sym("concede"), string(literal), string(in_reply_to)],
        SpeechAct::Commit {
            sentence_id,
            trigger,
            by,
            goal,
        } => {
            let trigger_arg = match by {
                // (by "<ts>" "<trigger>") — elephant-defined structured arg;
                // strings only, so no SPL/CBCL lexicon clash.
                Some(ts) => SExpr::List(vec![sym("by"), string(ts), string(trigger)]),
                None => string(trigger),
            };
            vec![
                sym("commit"),
                string(sentence_id),
                trigger_arg,
                string(goal),
            ]
        }
        SpeechAct::Request {
            request_id,
            addressee,
            trigger,
            goal,
        } => vec![
            sym("request"),
            string(request_id),
            string(addressee),
            string(trigger),
            string(goal),
        ],
        SpeechAct::Justify {
            conclusion_id,
            premises,
        } => vec![sym("justify"), string(conclusion_id), string(premises)],
    };
    SExpr::List(items)
}

/// Canonical CBCL text for a speech act (CON-003 wire shape).
pub fn build_wire(act: &SpeechAct, signer_did: &str, key_id: &str, ts_rfc3339: &str) -> String {
    let lang = SExpr::List(vec![
        sym("lang"),
        sym(super::dialect::DIALECT_NAME),
        act_sexpr(act),
    ]);
    let with_limits = SExpr::List(vec![sym("with-limits"), kw("ttl"), string("inf"), lang]);
    let signed = SExpr::List(vec![
        sym("signed"),
        kw("signer"),
        string(signer_did),
        kw("key"),
        string(key_id),
        kw("ts"),
        string(ts_rfc3339),
        with_limits,
    ]);
    cbcl_core::serializer::serialize(&signed)
}

// ── merge-time validation (REQ-022) ────────────────────────────────────

/// Why an entry was quarantined rather than admitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quarantine {
    BadSignature,
    UnknownSigner,
    CbclUnparseable(String),
    WrongShape(String),
    WrongDialect(String),
    UnknownPerformative(String),
    BadPayload(String),
    TheoryMismatch,
    WrongVersion(u16),
}

impl std::fmt::Display for Quarantine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Quarantine::BadSignature => write!(f, "signature verification failed"),
            Quarantine::UnknownSigner => write!(f, "signer DID unknown to this theory"),
            Quarantine::CbclUnparseable(e) => write!(f, "cbcl unparseable: {e}"),
            Quarantine::WrongShape(e) => write!(f, "wrong envelope shape: {e}"),
            Quarantine::WrongDialect(d) => write!(f, "wrong dialect: {d}"),
            Quarantine::UnknownPerformative(p) => write!(f, "unknown performative: {p}"),
            Quarantine::BadPayload(e) => write!(f, "bad payload: {e}"),
            Quarantine::TheoryMismatch => write!(f, "entry theory does not match corpus"),
            Quarantine::WrongVersion(v) => write!(f, "unsupported entry version {v}"),
        }
    }
}

/// Parse and structurally validate an Entry's CBCL text into a SpeechAct.
/// Signature verification is the caller's step (needs a key resolver);
/// this function is pure recognition.
pub fn parse_wire(cbcl_text: &str) -> Result<SpeechAct, Quarantine> {
    let sexpr =
        cbcl_parser::parse(cbcl_text).map_err(|e| Quarantine::CbclUnparseable(format!("{e:?}")))?;
    let msg = cbcl_parser::parse_message(&sexpr).map_err(Quarantine::CbclUnparseable)?;

    // signed → with-limits → lang cbcl-elephant → performative
    let Message::Wrapped {
        wrapper: WrapperType::Signed,
        content,
        ..
    } = msg
    else {
        return Err(Quarantine::WrongShape(
            "outermost must be (signed …)".into(),
        ));
    };
    let Message::Wrapped {
        wrapper: WrapperType::WithLimits,
        content,
        ..
    } = *content
    else {
        return Err(Quarantine::WrongShape(
            "second wrapper must be (with-limits …)".into(),
        ));
    };
    let Message::Dialect {
        dialect_name,
        inner: _,
    } = *content
    else {
        return Err(Quarantine::WrongShape(
            "third wrapper must be (lang …)".into(),
        ));
    };
    if dialect_name != super::dialect::DIALECT_NAME {
        return Err(Quarantine::WrongDialect(dialect_name));
    }

    // Re-read the raw performative list from the AST for exact arg shapes.
    // (Message::Simple splits content/params in a way that loses arity.)
    let inner_sexpr = inner_to_sexpr(&sexpr)?;
    parse_act(&inner_sexpr)
}

/// Walk the raw AST down to `(lang cbcl-elephant <act>)` and return <act>.
fn inner_to_sexpr(root: &SExpr) -> Result<SExpr, Quarantine> {
    fn find_lang(e: &SExpr) -> Option<&SExpr> {
        if let SExpr::List(items) = e {
            if let Some(SExpr::Atom(Atom::Symbol(head))) = items.first() {
                if head == "lang" && items.len() == 3 {
                    return Some(&items[2]);
                }
                // wrappers: inner message is the last element
                if head == "signed" || head == "with-limits" || head == "envelope" {
                    return items.last().and_then(find_lang);
                }
            }
        }
        None
    }
    find_lang(root)
        .cloned()
        .ok_or_else(|| Quarantine::WrongShape("no (lang …) form found".into()))
}

fn parse_act(e: &SExpr) -> Result<SpeechAct, Quarantine> {
    let SExpr::List(items) = e else {
        return Err(Quarantine::WrongShape("performative must be a list".into()));
    };
    let Some(SExpr::Atom(Atom::Symbol(head))) = items.first() else {
        return Err(Quarantine::WrongShape("performative head missing".into()));
    };
    let args = &items[1..];
    let str_at = |i: usize, what: &str| -> Result<String, Quarantine> {
        match args.get(i) {
            Some(SExpr::Atom(Atom::Str(s))) => Ok(s.clone()),
            other => Err(Quarantine::BadPayload(format!(
                "{head}: arg {i} ({what}) must be a string, got {other:?}"
            ))),
        }
    };
    let arity = |n: usize| -> Result<(), Quarantine> {
        if args.len() == n {
            Ok(())
        } else {
            Err(Quarantine::BadPayload(format!(
                "{head}: expected {n} args, got {}",
                args.len()
            )))
        }
    };

    match head.as_str() {
        "assert" => {
            arity(2)?;
            Ok(SpeechAct::Assert {
                sentence_id: str_at(0, "sentence-id")?,
                spl: str_at(1, "spl")?,
            })
        }
        "retract" => {
            arity(2)?;
            Ok(SpeechAct::Retract {
                target: str_at(0, "sentence-id")?,
                reason: str_at(1, "reason")?,
            })
        }
        "query" => {
            arity(2)?;
            let explain = matches!(args.get(1), Some(SExpr::Atom(Atom::Bool(true))));
            Ok(SpeechAct::Query {
                literal: str_at(0, "literal")?,
                explain,
            })
        }
        "concede" => {
            arity(2)?;
            Ok(SpeechAct::Concede {
                literal: str_at(0, "literal")?,
                in_reply_to: str_at(1, "in-reply-to")?,
            })
        }
        "commit" => {
            arity(3)?;
            let sentence_id = str_at(0, "sentence-id")?;
            let goal = match args.get(2) {
                Some(SExpr::Atom(Atom::Str(s))) => s.clone(),
                other => {
                    return Err(Quarantine::BadPayload(format!(
                        "commit: goal must be a string, got {other:?}"
                    )));
                }
            };
            let (trigger, by) = match args.get(1) {
                Some(SExpr::Atom(Atom::Str(s))) => (s.clone(), None),
                Some(SExpr::List(parts)) => match parts.as_slice() {
                    [
                        SExpr::Atom(Atom::Symbol(b)),
                        SExpr::Atom(Atom::Str(ts)),
                        SExpr::Atom(Atom::Str(trigger)),
                    ] if b == "by" => (trigger.clone(), Some(ts.clone())),
                    _ => {
                        return Err(Quarantine::BadPayload(
                            "commit: trigger must be \"<spl>\" or (by \"<ts>\" \"<spl>\")".into(),
                        ));
                    }
                },
                other => {
                    return Err(Quarantine::BadPayload(format!(
                        "commit: bad trigger arg {other:?}"
                    )));
                }
            };
            Ok(SpeechAct::Commit {
                sentence_id,
                trigger,
                by,
                goal,
            })
        }
        "request" => {
            arity(4)?;
            Ok(SpeechAct::Request {
                request_id: str_at(0, "request-id")?,
                addressee: str_at(1, "addressee")?,
                trigger: str_at(2, "trigger-conditions")?,
                goal: str_at(3, "goal")?,
            })
        }
        "justify" => {
            arity(2)?;
            Ok(SpeechAct::Justify {
                conclusion_id: str_at(0, "conclusion-id")?,
                premises: str_at(1, "premises")?,
            })
        }
        other => Err(Quarantine::UnknownPerformative(other.to_string())),
    }
}

/// Full merge-time validation of a deserialised Entry (REQ-022 steps 1–4).
/// `resolve` maps (signer DID, key id) → verifying key, from the theory's
/// known member documents.
pub fn validate_entry(
    entry: &Entry,
    expected_theory: &str,
    resolve: &dyn Fn(&str, &str) -> Option<ed25519_dalek::VerifyingKey>,
) -> Result<SpeechAct, Quarantine> {
    if entry.v != ENTRY_VERSION {
        return Err(Quarantine::WrongVersion(entry.v));
    }
    if entry.theory != expected_theory {
        return Err(Quarantine::TheoryMismatch);
    }
    let Some(pubkey) = resolve(&entry.signer, &entry.key_id) else {
        return Err(Quarantine::UnknownSigner);
    };
    if !entry.verify_sig(&pubkey) {
        return Err(Quarantine::BadSignature);
    }
    let act = parse_wire(&entry.cbcl)?;
    // Payload-level SPL recognition for asserts (REQ-022 step 4 / CON-001):
    if let SpeechAct::Assert { spl, .. } = &act {
        validate_assert_payload(spl)?;
    }
    Ok(act)
}

/// SPL payload restrictions (CON-001): must parse; no claims blocks
/// (provenance is envelope-derived, ADR-012).
pub fn validate_assert_payload(spl: &str) -> Result<(), Quarantine> {
    let theory =
        spindle_parser::parse_spl(spl).map_err(|e| Quarantine::BadPayload(format!("spl: {e}")))?;
    // ADR-012: inline claims blocks are rejected — a claims wrapper adds
    // source metadata to contained rules; detect by any rule carrying a
    // "source" meta property after a bare parse.
    for rule in theory.rules() {
        if let Some(meta) = theory.get_meta(&rule.label) {
            if meta.properties.contains_key("source") {
                return Err(Quarantine::BadPayload(
                    "inline (claims …) blocks are not allowed; provenance is envelope-derived"
                        .into(),
                ));
            }
        }
    }
    Ok(())
}

pub fn entry_to_json(entry: &Entry) -> String {
    serde_json::to_string(entry).expect("Entry serialisation is infallible")
}

pub fn entry_from_json(bytes: &str) -> AppResult<Entry> {
    serde_json::from_str(bytes).map_err(|e| AppError::Parse(format!("entry json: {e}")))
}

mod sig_hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex(sig))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let s = String::deserialize(d)?;
        unhex(&s).ok_or_else(|| serde::de::Error::custom("invalid signature hex"))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(s: &str) -> Option<[u8; 64]> {
        if s.len() != 128 || !s.is_ascii() {
            return None;
        }
        let mut out = [0u8; 64];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    fn hlc() -> Hlc {
        Hlc {
            wall_ms: 1_752_000_000_000,
            logical: 0,
            node_id: 42,
        }
    }

    fn make(act: &SpeechAct) -> Entry {
        Entry::create(
            "th-test",
            hlc(),
            "did:crdt:aa",
            "did:crdt:aa#key-0",
            act,
            "2026-07-11T00:00:00Z",
            &test_key(),
        )
    }

    fn resolver_ok(_: &str, _: &str) -> Option<ed25519_dalek::VerifyingKey> {
        Some(test_key().verifying_key())
    }

    /// TEST-027 (example half): serialize ∘ parse = id, and the wire
    /// roundtrips through cbcl-parser back to the same act.
    #[test]
    fn entry_roundtrip_all_performatives() {
        let acts = [
            SpeechAct::Assert {
                sentence_id: "s-1".into(),
                spl: "(given qa-signed)".into(),
            },
            SpeechAct::Retract {
                target: "s-1".into(),
                reason: "misread the logs".into(),
            },
            SpeechAct::Query {
                literal: "release-ready".into(),
                explain: true,
            },
            SpeechAct::Concede {
                literal: "docs-ready".into(),
                in_reply_to: "s-9".into(),
            },
            SpeechAct::Commit {
                sentence_id: "s-2".into(),
                trigger: "".into(),
                by: Some("2026-07-18T17:00:00Z".into()),
                goal: "legal-signed".into(),
            },
            SpeechAct::Commit {
                sentence_id: "s-3".into(),
                trigger: "(and qa-signed docs-ready)".into(),
                by: None,
                goal: "release-ready".into(),
            },
            SpeechAct::Request {
                request_id: "s-4".into(),
                addressee: "did:crdt:bb".into(),
                trigger: "".into(),
                goal: "legal-signed".into(),
            },
            SpeechAct::Justify {
                conclusion_id: "s-5".into(),
                premises: "(normally r1 bird flies)".into(),
            },
        ];
        for act in &acts {
            let entry = make(act);
            let json = entry_to_json(&entry);
            let back = entry_from_json(&json).unwrap();
            assert_eq!(&back, &entry);
            let parsed = validate_entry(&back, "th-test", &resolver_ok);
            match (act, parsed) {
                // Assert with non-SPL payloads would fail; ours are valid.
                (a, Ok(p)) => assert_eq!(&p, a),
                (a, Err(q)) => panic!("act {a:?} quarantined: {q}"),
            }
        }
    }

    /// TEST-022 negative-input rows.
    #[test]
    fn tampered_entry_is_quarantined() {
        let act = SpeechAct::Assert {
            sentence_id: "s-1".into(),
            spl: "(given x)".into(),
        };
        // bad signature
        let mut e = make(&act);
        e.sig[0] ^= 0xff;
        assert_eq!(
            validate_entry(&e, "th-test", &resolver_ok),
            Err(Quarantine::BadSignature)
        );
        // theory mismatch
        let e = make(&act);
        assert_eq!(
            validate_entry(&e, "other", &resolver_ok),
            Err(Quarantine::TheoryMismatch)
        );
        // unknown signer
        let e = make(&act);
        assert_eq!(
            validate_entry(&e, "th-test", &|_, _| None),
            Err(Quarantine::UnknownSigner)
        );
        // tampered cbcl → signature breaks first (covers content integrity)
        let mut e = make(&act);
        e.cbcl = e.cbcl.replace("(given x)", "(given y)");
        assert_eq!(
            validate_entry(&e, "th-test", &resolver_ok),
            Err(Quarantine::BadSignature)
        );
    }

    /// TEST-022: malformed SPL in an assert payload cannot be admitted.
    #[test]
    fn bad_spl_payload_quarantined() {
        let act = SpeechAct::Assert {
            sentence_id: "s-1".into(),
            spl: "(given (unbalanced".into(),
        };
        let e = make(&act);
        assert!(matches!(
            validate_entry(&e, "th-test", &resolver_ok),
            Err(Quarantine::BadPayload(_))
        ));
    }

    /// ADR-012: inline claims blocks rejected.
    #[test]
    fn inline_claims_rejected() {
        let act = SpeechAct::Assert {
            sentence_id: "s-1".into(),
            spl: "(claims agent:mallory (given trusted-fact))".into(),
        };
        let e = make(&act);
        assert!(matches!(
            validate_entry(&e, "th-test", &resolver_ok),
            Err(Quarantine::BadPayload(_))
        ));
    }

    /// TEST-022: core performatives inside the dialect are not speech acts.
    #[test]
    fn core_performative_inside_lang_rejected() {
        let cbcl = "(signed :signer \"d\" :key \"k\" :ts \"t\" (with-limits :ttl \"inf\" \
                    (lang cbcl-elephant (tell \"hi\"))))";
        assert!(matches!(
            parse_wire(cbcl),
            Err(Quarantine::UnknownPerformative(_))
        ));
    }

    #[test]
    fn sentence_id_is_stable_and_prefixed() {
        let a = Entry::sentence_id("th", "did:crdt:aa", hlc());
        let b = Entry::sentence_id("th", "did:crdt:aa", hlc());
        assert_eq!(a, b);
        assert!(a.starts_with("s-") && a.len() == 18);
        assert_ne!(a, Entry::sentence_id("th2", "did:crdt:aa", hlc()));
    }
}
