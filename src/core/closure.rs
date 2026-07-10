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
/// `did:crdt:<64hex>` → `agent:<first 16 hex>`; theory members can list the
/// mapping via `elephant theory members`.
pub fn source_atom(did: &str) -> String {
    let tail = did.rsplit(':').next().unwrap_or(did);
    let prefix: String = tail.chars().take(16).collect();
    format!("agent:{prefix}")
}

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
        let expected = if e.theory == genesis_sentinel {
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
    let mut spl = String::new();
    spl.push_str(local_trust_spl);
    spl.push('\n');
    for a in admitted.iter().filter(|a| !a.retracted) {
        if let SpeechAct::Assert {
            sentence_id,
            spl: payload,
        } = &a.act
        {
            let source = source_atom(&a.entry.signer);
            let at = rfc3339_from_ms(a.entry.hlc.wall_ms);
            spl.push_str(&format!(
                "(claims {source} :at \"{at}\" :id \"{sentence_id}\"\n  {payload})\n"
            ));
        }
    }

    // Synthetic strict rules for commitment triggers (ADR-007): the trigger
    // body's provability surfaces as a reserved literal per commitment.
    let commits: Vec<&Admitted> = admitted
        .iter()
        .filter(|a| matches!(a.act, SpeechAct::Commit { .. }))
        .collect();
    for a in &commits {
        if a.retracted {
            continue;
        }
        let SpeechAct::Commit {
            sentence_id,
            trigger,
            ..
        } = &a.act
        else {
            unreachable!()
        };
        if !trigger.trim().is_empty() {
            spl.push_str(&format!(
                "(always __ct-{sentence_id} {trigger} {TRIG_PREFIX}{sentence_id})\n"
            ));
        }
    }

    // 5–6: parse, reason at `now`, weight by trust.
    let theory = spindle_parser::parse_spl(&spl)
        .map_err(|e| AppError::Reasoner(format!("assembled theory unparseable: {e}")))?;
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
    for a in &commits {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::envelope::Hlc;

    fn key(seed: u8) -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
    }

    fn did(seed: u8) -> String {
        format!("did:crdt:{}", format!("{seed:02x}").repeat(32))
    }

    struct Fixture {
        entries: Vec<Entry>,
        counter: u64,
    }

    impl Fixture {
        fn new() -> Fixture {
            Fixture {
                entries: Vec::new(),
                counter: 0,
            }
        }

        fn add(&mut self, seed: u8, act: SpeechAct) -> String {
            self.counter += 1;
            let hlc = Hlc {
                wall_ms: 1_752_000_000_000 + self.counter,
                logical: 0,
                node_id: seed as u64,
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

        fn assert_spl(&mut self, seed: u8, spl: &str) -> String {
            let sid = self.next_sid(seed);
            let act = SpeechAct::Assert {
                sentence_id: sid.clone(),
                spl: spl.to_string(),
            };
            self.add(seed, act);
            sid
        }

        fn next_sid(&self, seed: u8) -> String {
            let hlc = Hlc {
                wall_ms: 1_752_000_000_000 + self.counter + 1,
                logical: 0,
                node_id: seed as u64,
            };
            Entry::sentence_id("th-x", &did(seed), hlc)
        }

        fn close(&self, trust: &str, now_ms: i64) -> Closure {
            let resolve = |d: &str, _k: &str| -> Option<ed25519_dalek::VerifyingKey> {
                (1u8..=9)
                    .find(|s| did(*s) == d)
                    .map(|s| key(s).verifying_key())
            };
            close(&self.entries, "th-x", "genesis", &resolve, trust, now_ms).unwrap()
        }
    }

    const NOW: i64 = 1_784_000_000_000; // ≈ 2026-07-13

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
        assert_eq!(
            format!("{src:?}").contains(&source_atom(&did(1))),
            true,
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
