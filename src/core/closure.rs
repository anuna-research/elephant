//! The closure pipeline (SPEC-001 CON-003): entries → conclusions +
//! commitment states. Pure — `now` is a parameter, never a syscall.
//!
//! Stages: validate/quarantine → E1 tombstone filter → claims-wrapped SPL
//! assembly (provenance from verified envelopes, ADR-012) → spindle parse +
//! reason (reference_time = now) → trust weighting → commitment states
//! (ADR-007, McCarthy's existence axiom).

use crate::core::envelope::{Entry, Quarantine, SpeechAct};
use crate::errors::{AppError, AppResult};
use spindle_core::conclusion::{Conclusion, ConclusionType};
use spindle_core::pipeline::PrepareOptions;
use spindle_core::temporal::TimePoint;
use spindle_core::theory::Theory;
use spindle_core::trust::WeightedConclusion;

/// An admitted entry with its parsed act and derived ids.
#[derive(Debug, Clone)]
pub struct Admitted {
    pub entry: Entry,
    pub act: SpeechAct,
    /// The act's own sentence/request id when it carries one.
    pub sid: Option<String>,
    /// True when an E1-valid retraction excluded this entry from the theory.
    pub retracted: bool,
    /// True when this assert's explicit rule label was already defined by an
    /// earlier entry, so it was dropped from closure (first-by-hlc wins).
    pub label_shadowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitmentPhase {
    Retracted,
    Pending,
    Outstanding,
    Fulfilled,
    Violated,
}

impl std::fmt::Display for CommitmentPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CommitmentPhase::Retracted => "retracted",
            CommitmentPhase::Pending => "pending",
            CommitmentPhase::Outstanding => "outstanding",
            CommitmentPhase::Fulfilled => "fulfilled",
            CommitmentPhase::Violated => "violated",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone)]
pub struct CommitmentState {
    pub id: String,
    pub by_agent: String,
    pub trigger: String,
    pub deadline: Option<String>,
    pub goal: String,
    pub phase: CommitmentPhase,
}

pub struct Closure {
    /// Underlying spindle theory (for explain/why-not/what-if follow-ups).
    pub theory: Theory,
    pub conclusions: Vec<Conclusion>,
    pub weighted: Vec<WeightedConclusion>,
    pub commitments: Vec<CommitmentState>,
    pub admitted: Vec<Admitted>,
    pub quarantined: Vec<(Entry, Quarantine)>,
}

/// Deterministic SPL source atom for a signer DID (ADR-012).
/// `did:crdt:<64hex>` → `agent:<full method-specific id>`. The full id is
/// used, not a prefix: a truncated prefix would let a ~2^64 keypair grind
/// collide two DIDs onto one `agent:` atom and steal a trusted source's
/// weight, defeating the point of envelope-derived provenance.
pub fn source_atom(did: &str) -> String {
    let tail = did.rsplit(':').next().unwrap_or(did);
    format!("{AGENT_ATOM_PREFIX}{tail}")
}

/// Prefix of an envelope-derived provenance atom. Reserved: a consumer that
/// reads a rule's `source` metadata uses it to tell auto-attached provenance
/// apart from an author-written citation (SPEC-006 REQ-604).
pub const AGENT_ATOM_PREFIX: &str = "agent:";

fn rfc3339_from_ms(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// Reserved literal prefix for synthetic commitment-trigger rules.
const TRIG_PREFIX: &str = "__trig-";

/// Run the closure (CON-003).
///
/// `resolve` maps (signer DID, key id) → verifying key from the theory's
/// member documents; `local_trust_spl` is the agent's private trust policy
/// text (REQ-023); `now_ms` is the evaluation time (REQ-021: an argument).
pub fn close(
    entries: &[Entry],
    theory_id: &str,
    genesis_sentinel: &str,
    resolve: &dyn Fn(&str, &str) -> Option<ed25519_dalek::VerifyingKey>,
    local_trust_spl: &str,
    now_ms: i64,
) -> AppResult<Closure> {
    // 1–2: validate every entry; quarantine failures.
    let mut admitted: Vec<Admitted> = Vec::new();
    let mut quarantined: Vec<(Entry, Quarantine)> = Vec::new();
    for e in entries {
        // The genesis sentinel is honoured ONLY for the entry that actually
        // is the genesis — i.e. whose canonical bytes hash to the theory id.
        // Without this check, any member could mint entries with
        // theory="genesis" that are bound to no theory and thus replay into
        // every theory they belong to (cross-theory binding hole).
        let is_genesis = crate::core::envelope::is_genesis(e, theory_id, genesis_sentinel);
        let expected = if is_genesis {
            genesis_sentinel
        } else {
            theory_id
        };
        match crate::core::envelope::validate_entry(e, expected, resolve) {
            Ok(act) => {
                let sid = match &act {
                    SpeechAct::Assert { sentence_id, .. }
                    | SpeechAct::Commit { sentence_id, .. } => Some(sentence_id.clone()),
                    SpeechAct::Request { request_id, .. } => Some(request_id.clone()),
                    _ => None,
                };
                admitted.push(Admitted {
                    entry: e.clone(),
                    act,
                    sid,
                    retracted: false,
                    label_shadowed: false,
                });
            }
            Err(q) => quarantined.push((e.clone(), q)),
        }
    }

    // 3: E1 tombstone filter — a retraction excludes its target iff the
    // signers match; foreign retractions stay in the corpus, inert.
    let retractions: Vec<(String, String)> = admitted
        .iter()
        .filter_map(|a| match &a.act {
            SpeechAct::Retract { target, .. } => Some((target.clone(), a.entry.signer.clone())),
            _ => None,
        })
        .collect();
    for a in admitted.iter_mut() {
        if let Some(sid) = &a.sid {
            if retractions
                .iter()
                .any(|(target, by)| target == sid && *by == a.entry.signer)
            {
                a.retracted = true;
            }
        }
    }

    // 4: assemble the SPL theory text. Local trust first (unsourced policy
    // directives), then one claims block per active assert.
    //
    // Rule labels share one namespace across the whole theory (hence's
    // cross-entry references rely on it), so two entries defining the same
    // explicit label would make the assembled SPL unparseable — a
    // member-triggerable denial of service. We keep the FIRST definition by
    // corpus order (hlc, signer — `admitted` is already in that order) and
    // drop later redefinitions from closure. They remain in the corpus and
    // the journal (REQ-016); only their effect on this closure is suppressed.
    let mut seen_labels: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Each block carries the admitted index it came from, so if the block
    // turns out to be toxic in combination (label collision the cheap
    // explicit-label dedup missed) it can be quarantined per-entry rather
    // than bricking the whole theory.
    let mut blocks: Vec<(usize, String)> = Vec::new();
    for (i, a) in admitted.iter_mut().enumerate() {
        if a.retracted {
            continue;
        }
        let (sentence_id, payload) = match &a.act {
            SpeechAct::Assert {
                sentence_id,
                spl: payload,
            } => (sentence_id.clone(), payload.clone()),
            _ => continue,
        };
        if let Some(label) = explicit_rule_label(&payload) {
            if !seen_labels.insert(label) {
                a.label_shadowed = true;
                continue;
            }
        }
        let source = source_atom(&a.entry.signer);
        let at = rfc3339_from_ms(a.entry.hlc.wall_ms);
        blocks.push((
            i,
            format!("(claims {source} :at \"{at}\" :id \"{sentence_id}\"\n  {payload})\n"),
        ));
    }

    // Synthetic strict rules for commitment triggers (ADR-007): the trigger
    // body's provability surfaces as a reserved literal per commitment.
    for (i, a) in admitted.iter().enumerate() {
        if a.retracted {
            continue;
        }
        if let SpeechAct::Commit {
            sentence_id,
            trigger,
            ..
        } = &a.act
        {
            if !trigger.trim().is_empty() {
                blocks.push((
                    i,
                    format!("(always __ct-{sentence_id} {trigger} {TRIG_PREFIX}{sentence_id})\n"),
                ));
            }
        }
    }

    // 5: parse. Fast path: assemble everything and parse once. On failure
    // (a residual label collision, or a payload spindle rejects only after
    // claims-wrapping), fall back to adding blocks one at a time and
    // quarantining any that break the parse — so a single toxic entry
    // fails closed per-entry (REQ-022) instead of wedging the whole theory.
    let assemble = |base: &str, blocks: &[(usize, String)]| -> String {
        let mut s =
            String::with_capacity(base.len() + blocks.iter().map(|(_, b)| b.len()).sum::<usize>());
        s.push_str(base);
        s.push('\n');
        for (_, b) in blocks {
            s.push_str(b);
        }
        s
    };
    let mut fallback_rejected: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let (spl, theory) = match spindle_parser::parse_spl(&assemble(local_trust_spl, &blocks)) {
        Ok(theory) => (assemble(local_trust_spl, &blocks), theory),
        Err(_) => {
            // Incremental fail-closed rebuild.
            let mut acc = String::from(local_trust_spl);
            acc.push('\n');
            // The trust preamble alone must parse; if it does not, that is a
            // configuration error the caller owns, surfaced as before.
            spindle_parser::parse_spl(&acc)
                .map_err(|e| AppError::Reasoner(format!("local trust policy unparseable: {e}")))?;
            for (idx, block) in blocks {
                let candidate = format!("{acc}{block}");
                if spindle_parser::parse_spl(&candidate).is_ok() {
                    acc = candidate;
                } else {
                    // This entry is toxic in combination — quarantine it.
                    fallback_rejected.insert(idx);
                    quarantined.push((
                        admitted[idx].entry.clone(),
                        Quarantine::BadPayload(
                            "statement collides with the assembled theory (e.g. duplicate rule label)"
                                .into(),
                        ),
                    ));
                }
            }
            let theory = spindle_parser::parse_spl(&acc)
                .map_err(|e| AppError::Reasoner(format!("assembled theory unparseable: {e}")))?;
            (acc, theory)
        }
    };
    let _ = &spl;
    // A fallback-rejected entry is quarantined, not merely dropped from the
    // parse: remove it from admission so it emits no commitment state below
    // and no duplicate journal row.
    if !fallback_rejected.is_empty() {
        let mut i = 0;
        admitted.retain(|_| {
            let keep = !fallback_rejected.contains(&i);
            i += 1;
            keep
        });
    }
    let opts = PrepareOptions {
        reference_time: Some(TimePoint::from_millis(now_ms)),
        ..Default::default()
    };
    let conclusions = spindle_core::reason::reason_with_options(&theory, opts)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    let weighted = spindle_core::pipeline::compute_weighted_conclusions(
        &conclusions,
        &theory,
        theory.trust_policy(),
        Some(TimePoint::from_millis(now_ms)),
    );

    // 7: commitment states (REQ-015).
    let provable = |lit_text: &str| -> bool {
        let Some(canon) = normalize_literal(lit_text) else {
            return false;
        };
        conclusions.iter().any(|c| {
            c.conclusion_type.is_positive() && !c.literal.negation && c.literal.to_spl() == canon
        })
    };
    let mut commitments = Vec::new();
    for a in admitted
        .iter()
        .filter(|a| matches!(a.act, SpeechAct::Commit { .. }))
    {
        let SpeechAct::Commit {
            sentence_id,
            trigger,
            by,
            goal,
        } = &a.act
        else {
            unreachable!()
        };
        let phase = if a.retracted {
            CommitmentPhase::Retracted
        } else {
            let trigger_holds =
                trigger.trim().is_empty() || provable(&format!("{TRIG_PREFIX}{sentence_id}"));
            let goal_holds = provable(goal.trim());
            let deadline_passed = by
                .as_deref()
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                .map(|dl| dl.timestamp_millis() < now_ms)
                .unwrap_or(false);
            if !trigger_holds {
                CommitmentPhase::Pending
            } else if goal_holds {
                CommitmentPhase::Fulfilled
            } else if deadline_passed {
                CommitmentPhase::Violated
            } else {
                CommitmentPhase::Outstanding
            }
        };
        commitments.push(CommitmentState {
            id: sentence_id.clone(),
            by_agent: a.entry.signer.clone(),
            trigger: trigger.clone(),
            deadline: by.clone(),
            goal: goal.clone(),
            phase,
        });
    }

    Ok(Closure {
        theory,
        conclusions,
        weighted,
        commitments,
        admitted,
        quarantined,
    })
}

/// The explicit rule label an SPL payload defines, if any. Labels are the
/// token right after `always`/`normally`/`except`; `given`/`prefer`/`trusts`
/// and friends define none. Returns None when the rule is unlabelled (the
/// reasoner will auto-number it, and auto labels never collide across
/// entries). Parsing here reuses the single SPL recogniser.
fn explicit_rule_label(payload: &str) -> Option<String> {
    // Fast path: only `always`/`normally`/`except` forms can carry an
    // explicit rule label. Facts and everything else skip the parse — this
    // keeps assembly close to a single parse of the whole theory (NFR-001).
    let head = payload.trim_start().trim_start_matches('(').trim_start();
    if !(head.starts_with("always") || head.starts_with("normally") || head.starts_with("except")) {
        return None;
    }
    let theory = spindle_parser::parse_spl(payload).ok()?;
    for rule in theory.rules() {
        // spindle auto-numbers unlabelled rules (f1/s1/r1/d1…); an explicit
        // label is one that appears verbatim as a token in the payload.
        let label = &rule.label;
        if payload
            .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
            .any(|tok| tok == label)
        {
            return Some(label.clone());
        }
    }
    None
}

/// Render a literal for display and for pasting back into a query command.
/// `to_spl` already emits the single `(not …)` wrapper for a negated literal,
/// so a negated form is returned verbatim — the inner parens are load-bearing.
/// A positive literal has its outer parens stripped for readability; the
/// flat-form `(given <atom> <args…>)` sugar still accepts the result, so the
/// query commands round-trip it. Shared by `status`, `trace`, and the closure
/// fingerprint so the display form the fingerprint hashes cannot drift from the
/// one `status --json` prints (#20).
pub fn literal_display(l: &spindle_core::literal::Literal) -> String {
    let spl = l.to_spl();
    if l.negation {
        spl
    } else {
        spl.strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .map(str::to_string)
            .unwrap_or(spl)
    }
}

/// Canonical `to_spl` rendering of a literal given as user text
/// ("release-ready" → "(release-ready)"), via the single SPL recogniser.
pub fn normalize_literal(text: &str) -> Option<String> {
    let t = spindle_parser::parse_spl(&format!("(given {})", text.trim())).ok()?;
    let rule = t.rules().next()?;
    rule.head.first().map(|l| l.to_spl())
}

/// Conclusions filtered for presentation: skip reserved synthetic literals.
pub fn presentable(conclusions: &[Conclusion]) -> impl Iterator<Item = &Conclusion> {
    conclusions.iter().filter(|c| {
        !c.literal.to_spl().contains("__trig-") && !c.literal.to_spl().contains("__ct-")
    })
}

pub fn tag_of(c: &ConclusionType) -> &'static str {
    c.symbol()
}

/// Version of the canonical proof-state mapping (#26). Bumped only if a name,
/// `positive`, or `level` value changes for an existing tag; adding fields is
/// backwards-compatible and does not bump it.
pub const PROOF_STATE_MAP_VERSION: u32 = 1;

/// The plain-language reading of a spindle proof tag (#26).
pub struct ProofState {
    /// Canonical snake_case name — the engine's own `ConclusionType` variant,
    /// lower-cased. Authoritative, not a paraphrase.
    pub name: &'static str,
    /// Whether the literal is (definitely or defeasibly) provable.
    pub positive: bool,
    /// Proof strength: `definite` (strict) or `defeasible`.
    pub level: &'static str,
}

/// Canonical, versioned reading of a compact `+D/+d/-D/-d` tag (#26).
///
/// The compact symbols are expert shorthand; in the grounded run they were
/// paraphrased inconsistently (strict, blocked, refuted, not provable…), and a
/// single prose gloss for a negative tag hid *why* it was negative. This is the
/// one authoritative mapping, shared by every command that reports a tag so API
/// clients never have to invent their own. `reason_class` (complement proved
/// vs. ambiguity vs. missing support) is a separate, best-effort diagnostic —
/// see `why-not` — and is deliberately not folded in here, because it is not
/// cheaply derivable from the tag alone.
pub fn proof_state(t: ConclusionType) -> ProofState {
    match t {
        ConclusionType::DefinitelyProvable => ProofState {
            name: "definitely_provable",
            positive: true,
            level: "definite",
        },
        ConclusionType::DefeasiblyProvable => ProofState {
            name: "defeasibly_provable",
            positive: true,
            level: "defeasible",
        },
        ConclusionType::DefinitelyNotProvable => ProofState {
            name: "definitely_not_provable",
            positive: false,
            level: "definite",
        },
        ConclusionType::DefeasiblyNotProvable => ProofState {
            name: "defeasibly_not_provable",
            positive: false,
            level: "defeasible",
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::envelope::Hlc;

    fn key(seed: u8) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
    }

    /// The HLC node id `validate_entry` requires: the one derived from the
    /// signer's key, not an arbitrary label.
    fn node(seed: u8) -> u64 {
        did_crdt::core::validate::node_id_from_pubkey(key(seed).verifying_key().as_bytes())
    }

    fn did(seed: u8) -> String {
        format!("did:crdt:{}", format!("{seed:02x}").repeat(32))
    }

    pub(crate) struct Fixture {
        entries: Vec<Entry>,
        counter: u64,
    }

    impl Fixture {
        pub(crate) fn new() -> Fixture {
            Fixture {
                entries: Vec::new(),
                counter: 0,
            }
        }

        pub(crate) fn add(&mut self, seed: u8, act: SpeechAct) -> String {
            self.counter += 1;
            let hlc = Hlc {
                wall_ms: 1_752_000_000_000 + self.counter,
                logical: 0,
                node_id: node(seed),
            };
            let e = Entry::create(
                "th-x",
                hlc,
                &did(seed),
                &format!("{}#key-0", did(seed)),
                &act,
                "2026-07-11T00:00:00Z",
                &key(seed),
            );
            let sid = crate::core::envelope::Entry::sentence_id("th-x", &did(seed), hlc);
            self.entries.push(e);
            sid
        }

        pub(crate) fn assert_spl(&mut self, seed: u8, spl: &str) -> String {
            let sid = self.next_sid(seed);
            let act = SpeechAct::Assert {
                sentence_id: sid.clone(),
                spl: spl.to_string(),
            };
            self.add(seed, act);
            sid
        }

        /// A `Commit` speech act, for exercising commitment-dependent
        /// projections (SPEC-006 REQ-601) without hand-building the act.
        pub(crate) fn commit(
            &mut self,
            seed: u8,
            trigger: &str,
            by: Option<&str>,
            goal: &str,
        ) -> String {
            let sid = self.next_sid(seed);
            let act = SpeechAct::Commit {
                sentence_id: sid.clone(),
                trigger: trigger.to_string(),
                by: by.map(str::to_string),
                goal: goal.to_string(),
            };
            self.add(seed, act);
            sid
        }

        pub(crate) fn next_sid(&self, seed: u8) -> String {
            let hlc = Hlc {
                wall_ms: 1_752_000_000_000 + self.counter + 1,
                logical: 0,
                node_id: node(seed),
            };
            Entry::sentence_id("th-x", &did(seed), hlc)
        }

        pub(crate) fn close(&self, trust: &str, now_ms: i64) -> Closure {
            let resolve = |d: &str, _k: &str| -> Option<ed25519_dalek::VerifyingKey> {
                (1u8..=9)
                    .find(|s| did(*s) == d)
                    .map(|s| key(s).verifying_key())
            };
            close(&self.entries, "th-x", "genesis", &resolve, trust, now_ms).unwrap()
        }
    }

    const NOW: i64 = 1_784_000_000_000; // ≈ 2026-07-13

    /// S3 fix (adversarial review): a statement valid alone but toxic in
    /// combination (a label colliding with spindle's auto-numbering) must be
    /// quarantined per-entry, never brick the whole closure (REQ-022).
    #[test]
    fn toxic_statement_is_quarantined_not_fatal() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given a)");
        f.assert_spl(1, "(normally f1 a b)");
        f.assert_spl(1, "(normally s1 a c)");
        f.assert_spl(1, "(given d)");
        let c = f.close("", NOW);
        assert!(
            c.conclusions
                .iter()
                .any(|x| x.literal.name() == "a" && x.conclusion_type.is_positive()),
            "uncontested fact must still derive"
        );
        assert!(
            c.conclusions
                .iter()
                .any(|x| x.literal.name() == "d" && x.conclusion_type.is_positive()),
            "later uncontested fact must still derive"
        );
    }

    /// S3 fix: an entry whose `theory` field is the genesis sentinel but
    /// which is NOT the real genesis is quarantined (cross-theory binding).
    #[test]
    fn spoofed_genesis_is_quarantined() {
        use crate::core::envelope::{Entry, Hlc};
        let k = key(1);
        let hlc = Hlc {
            wall_ms: 1_784_000_000_005,
            logical: 0,
            node_id: node(1),
        };
        let spoof = Entry::create(
            "genesis",
            hlc,
            &did(1),
            &format!("{}#key-0", did(1)),
            &SpeechAct::Assert {
                sentence_id: "s-spoof".into(),
                spl: "(given injected)".into(),
            },
            "2026-07-11T00:00:00Z",
            &k,
        );
        let resolve = |d: &str, _k: &str| (d == did(1)).then(|| k.verifying_key());
        let c = close(&[spoof], "th-real", "genesis", &resolve, "", NOW).unwrap();
        assert_eq!(
            c.quarantined.len(),
            1,
            "spoofed genesis must be quarantined"
        );
        assert!(
            !c.conclusions.iter().any(|x| x.literal.name() == "injected"),
            "spoofed-genesis statement must not influence closure"
        );
    }

    /// A commit whose synthetic trigger block breaks the assembled parse is
    /// quarantined AND excluded from admission: no commitment state may be
    /// emitted for it, and it must not appear as both admitted and
    /// quarantined in the journal.
    #[test]
    fn fallback_rejected_commit_is_fully_excluded() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given a)");
        let sid = f.next_sid(1);
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: sid.clone(),
                trigger: ") not spl (".into(), // wrecks the synthetic rule
                by: None,
                goal: "a".into(),
            },
        );
        let c = f.close("", NOW);
        assert_eq!(c.quarantined.len(), 1, "toxic commit is quarantined");
        assert!(
            c.commitments.iter().all(|s| s.id != sid),
            "a quarantined commit must not emit a commitment state"
        );
        assert!(
            !c.admitted
                .iter()
                .any(|a| a.sid.as_deref() == Some(sid.as_str())),
            "a quarantined commit must be removed from admission"
        );
        assert!(
            c.conclusions
                .iter()
                .any(|x| x.literal.name() == "a" && x.conclusion_type.is_positive()),
            "unrelated facts still derive"
        );
    }

    /// Basic derivation with provenance-wrapped claims (penguin-flavoured).
    #[test]
    fn derives_with_envelope_provenance() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given qa-signed)");
        f.assert_spl(2, "(given legal-signed)");
        f.assert_spl(
            2,
            "(normally r-ready (and qa-signed legal-signed) release-ready)",
        );
        let c = f.close("", NOW);
        assert!(c.quarantined.is_empty());
        let ready = c
            .conclusions
            .iter()
            .find(|x| {
                x.literal.name() == "release-ready"
                    && !x.literal.negation
                    && x.conclusion_type.is_positive()
            })
            .expect("release-ready derivable");
        assert_eq!(ready.conclusion_type, ConclusionType::DefeasiblyProvable);
        // Provenance flows from the envelope: the fact's rule label carries
        // the signer's source atom.
        let label = c
            .conclusions
            .iter()
            .find(|x| {
                x.literal.name() == "qa-signed"
                    && !x.literal.negation
                    && x.conclusion_type.is_positive()
            })
            .and_then(|x| x.rule_label.clone())
            .expect("fact has a rule label");
        let meta = c.theory.get_meta(&label).expect("claims meta present");
        let src = meta.properties.get("source").expect("source recorded");
        assert!(
            format!("{src:?}").contains(&source_atom(&did(1))),
            "source must be the envelope signer, got {src:?}"
        );
    }

    /// TEST-006: own retraction removes the statement; foreign is inert.
    #[test]
    fn e1_tombstone_filter() {
        let mut f = Fixture::new();
        let sid = f.assert_spl(1, "(given qa-signed)");
        // Foreign retract (signer 2): inert.
        f.add(
            2,
            SpeechAct::Retract {
                target: sid.clone(),
                reason: "not mine to take".into(),
            },
        );
        let c = f.close("", NOW);
        assert!(
            c.conclusions.iter().any(|x| x.literal.name() == "qa-signed"
                && !x.literal.negation
                && x.conclusion_type.is_positive()),
            "foreign retract must not remove the fact"
        );
        // Own retract (signer 1): effective.
        f.add(
            1,
            SpeechAct::Retract {
                target: sid,
                reason: "misread".into(),
            },
        );
        let c = f.close("", NOW);
        assert!(
            !c.conclusions.iter().any(|x| x.literal.name() == "qa-signed"
                && !x.literal.negation
                && x.conclusion_type.is_positive()),
            "own retract must remove the fact"
        );
        // The corpus never shrank; the entry is admitted + marked.
        assert!(c.admitted.iter().any(|a| a.retracted));
    }

    /// TEST-021 (kernel): closure is order-independent.
    #[test]
    fn closure_is_order_independent() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given bird)");
        f.assert_spl(2, "(normally r1 bird flies)");
        f.assert_spl(3, "(normally r2 penguin (not flies))");
        f.assert_spl(1, "(given penguin)");
        f.assert_spl(2, "(prefer r2 r1)");
        let base = f.close("", NOW);
        let mut tags: Vec<(String, bool, &str)> = presentable(&base.conclusions)
            .map(|c| {
                (
                    c.literal.name().to_string(),
                    c.literal.negation,
                    c.conclusion_type.symbol(),
                )
            })
            .collect();
        tags.sort();

        let mut reversed = Fixture::new();
        reversed.entries = f.entries.iter().rev().cloned().collect();
        let again = reversed.close("", NOW);
        let mut tags2: Vec<(String, bool, &str)> = presentable(&again.conclusions)
            .map(|c| {
                (
                    c.literal.name().to_string(),
                    c.literal.negation,
                    c.conclusion_type.symbol(),
                )
            })
            .collect();
        tags2.sort();
        assert_eq!(tags, tags2);
        // And the penguin does not fly.
        assert!(
            tags.iter()
                .any(|(l, neg, t)| l == "flies" && *neg && *t == "+d")
        );
    }

    /// TEST-015: the five commitment phases.
    #[test]
    fn commitment_phases() {
        let now = NOW;
        let past = "2026-07-01T00:00:00Z"; // before NOW
        let future = "2036-01-01T00:00:00Z";

        // pending: trigger not provable
        let mut f = Fixture::new();
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: f.next_sid(1),
                trigger: "qa-signed".into(),
                by: None,
                goal: "legal-signed".into(),
            },
        );
        assert_eq!(
            f.close("", now).commitments[0].phase,
            CommitmentPhase::Pending
        );

        // outstanding: trigger true (empty), goal not provable, future deadline
        let mut f = Fixture::new();
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: f.next_sid(1),
                trigger: "".into(),
                by: Some(future.into()),
                goal: "legal-signed".into(),
            },
        );
        assert_eq!(
            f.close("", now).commitments[0].phase,
            CommitmentPhase::Outstanding
        );

        // violated: deadline passed, goal not provable
        let mut f = Fixture::new();
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: f.next_sid(1),
                trigger: "".into(),
                by: Some(past.into()),
                goal: "legal-signed".into(),
            },
        );
        assert_eq!(
            f.close("", now).commitments[0].phase,
            CommitmentPhase::Violated
        );

        // fulfilled: goal provable (even though deadline passed — evidence wins)
        let mut f = Fixture::new();
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: f.next_sid(1),
                trigger: "".into(),
                by: Some(past.into()),
                goal: "legal-signed".into(),
            },
        );
        f.assert_spl(2, "(given legal-signed)");
        assert_eq!(
            f.close("", now).commitments[0].phase,
            CommitmentPhase::Fulfilled
        );

        // retracted: own retract of the commit
        let mut f = Fixture::new();
        let sid = f.next_sid(1);
        f.add(
            1,
            SpeechAct::Commit {
                sentence_id: sid.clone(),
                trigger: "".into(),
                by: None,
                goal: "legal-signed".into(),
            },
        );
        f.add(
            1,
            SpeechAct::Retract {
                target: sid,
                reason: "circumstances changed".into(),
            },
        );
        assert_eq!(
            f.close("", now).commitments[0].phase,
            CommitmentPhase::Retracted
        );
    }

    /// REQ-023: local trust changes weights without touching the corpus.
    #[test]
    fn local_trust_weights() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given qa-signed)");
        let trust = format!(
            "(trusts {} 0.9)\n(threshold act 0.5)\n",
            source_atom(&did(1))
        );
        let c = f.close(&trust, NOW);
        let w = c
            .weighted
            .iter()
            .find(|w| w.literal.name() == "qa-signed" && !w.literal.negation)
            .expect("weighted conclusion");
        assert!(
            w.degree > 0.8,
            "degree {} should reflect 0.9 trust",
            w.degree
        );

        // Without local trust the same fact carries default weight.
        let c2 = f.close("", NOW);
        let w2 = c2
            .weighted
            .iter()
            .find(|w| w.literal.name() == "qa-signed" && !w.literal.negation)
            .unwrap();
        assert!(w2.degree < w.degree);
    }

    /// Quarantined entries never influence conclusions (TEST-022 neg-output).
    #[test]
    fn quarantined_entries_are_inert() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given qa-signed)");
        // Signer 9 is unknown to the resolver in `close` (resolver maps 1..=9;
        // use an unresolvable did by tampering the signature instead).
        let mut bad = f.entries[0].clone();
        bad.sig[0] ^= 1;
        f.entries.push(bad);
        let c = f.close("", NOW);
        assert_eq!(c.quarantined.len(), 1);
        assert_eq!(
            c.conclusions
                .iter()
                .filter(|x| x.literal.name() == "qa-signed"
                    && !x.literal.negation
                    && x.conclusion_type == ConclusionType::DefeasiblyProvable)
                .count(),
            1
        );
    }
}
