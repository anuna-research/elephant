//! Task-discovery projection (SPEC-006 CON-602): closure + commitment states
//! → the CON-601 selection object. Pure — no signer, store, daemon client,
//! clock, subprocess, or filesystem path, and no second reasoner pass.
//!
//! Tasks are identified by predicate argument (SPEC-006 ADR-604), never by a
//! hyphen suffix on an atom name.

use std::collections::{BTreeMap, BTreeSet};

use spindle_core::conclusion::Conclusion;
use spindle_core::literal::Literal;
use spindle_core::mode::Mode;
use spindle_core::temporal::Temporal;
use spindle_core::term::Term;
use spindle_core::theory::{MetaValue, Theory};

use crate::core::closure::{Closure, CommitmentPhase, CommitmentState, normalize_literal};

/// Predicate symbols the projection recognises (SPEC-006 CON-601).
const P_TASK: (&str, usize) = ("task", 1);
const P_READY: (&str, usize) = ("ready", 1);
const P_COMPLETED: (&str, usize) = ("completed", 1);
const P_DESCRIPTION: (&str, usize) = ("task-description", 2);
const P_ACCEPTANCE: (&str, usize) = ("task-acceptance", 2);

/// Why a ready task was not offered. Ordered as SPEC-006 CON-601 lists them:
/// when several apply the first wins, making the diagnostic deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Completed,
    OutstandingCommitment,
    MissingTaskDescription,
    MissingTaskAcceptance,
    MissingReadyRule,
    MissingReadySource,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Completed => "completed",
            Reason::OutstandingCommitment => "outstanding-commitment",
            Reason::MissingTaskDescription => "missing-task-description",
            Reason::MissingTaskAcceptance => "missing-task-acceptance",
            Reason::MissingReadyRule => "missing-ready-rule",
            Reason::MissingReadySource => "missing-ready-source",
        }
    }
}

/// One offered task, carrying its documentation and provenance (SPEC-006 REQ-605).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub task: String,
    pub description: String,
    pub acceptance: String,
    /// Canonical `to_spl()` form, e.g. `(ready models)` — a machine operand,
    /// never `literal_display`'s paren-stripped human form (SPEC-006 CON-601).
    pub ready_literal: String,
    /// Canonical `(completed <task>)` — the goal of the suggested promise.
    /// Rendered here, in the core, so the shell never rebuilds it by string
    /// interpolation and can never disagree with the exclusion test that
    /// decided this task was still open (SPEC-006 REQ-601).
    pub promise_goal: String,
    /// The *template* label; a grounded instance `{template}_{n}` carries no
    /// metadata, so reporting one would yield an inert `describe` (REQ-605).
    pub ready_rule: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithheldItem {
    pub task: String,
    pub reason: Reason,
}

/// The CON-601 selection object, both arrays sorted bytewise by task.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub next: Vec<Candidate>,
    pub withheld: Vec<WithheldItem>,
}

/// Recover the template label a grounded rule instance came from.
///
/// Spindle renames each grounded instance of a quantified rule to
/// `{template}_{n}` and keeps only the template (with its body, and its
/// metadata) in the theory, so reporting the instance name would produce a
/// `describe` that resolves nothing (SPEC-006 REQ-605).
///
/// Resolution is **existence-driven**, mirroring spindle's own
/// `explanation::resolve_rule`: the label is tried as-is first, and a trailing
/// `_<digits>` run is stripped only until a prefix names a real rule. Blind
/// stripping would rewrite a rule genuinely named `phase_2` to `phase` and
/// mis-attribute its readiness.
///
/// Returns `None` when no prefix names a rule in this theory — the caller
/// reports that as `missing-ready-rule` rather than guessing.
pub fn resolve_template<'a>(theory: &Theory, label: &'a str) -> Option<&'a str> {
    if theory.get_rule(label).is_some() {
        return Some(label);
    }
    let mut cur = label;
    while let Some((prefix, suffix)) = cur.rsplit_once('_') {
        if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        if theory.get_rule(prefix).is_some() {
            return Some(prefix);
        }
        cur = prefix;
    }
    None
}

/// A positive, non-negated, unqualified conclusion matching `(functor arg0 …)`
/// at the declared arity, with arg 0 a symbol. Returns the arg-0 symbol.
///
/// Rejects a differently-arity homonym and a non-symbol arg 0, per SPEC-006
/// CON-601: such a conclusion is outside the convention and is ignored rather
/// than coerced.
///
/// Also rejects a modal or temporally-bounded literal. `Mode` and `Temporal`
/// sit *alongside* the name in spindle, so `(must (ready m))` and
/// `(during (ready m) …)` both report name `ready` at arity 1. A theory that
/// derives only the deontic `(must (ready m))` does not derive `(ready m)`,
/// and offering `m` would emit a `ready_literal` the reasoner cannot prove —
/// the REQ-605 receipt would contradict the candidate.
fn match_arg0<'a>(c: &'a Conclusion, (functor, arity): (&str, usize)) -> Option<&'a str> {
    if !c.conclusion_type.is_positive() || c.literal.negation {
        return None;
    }
    if !c.literal.mode.is_empty()
        || !c.literal.temporal.is_empty()
        || c.literal.temporal_expr.is_some()
        || c.literal.interval_var.is_some()
    {
        return None;
    }
    if c.literal.name() != functor {
        return None;
    }
    let args = c.literal.predicate_args();
    if args.len() != arity {
        return None;
    }
    match args.first() {
        Some(Term::Symbol(s)) => Some(spindle_core::intern::resolve(*s)),
        _ => None,
    }
}

/// Canonical `to_spl()` rendering of `(functor <task>)`.
///
/// Built through `Literal`, never by string interpolation. A task identifier
/// is any `Term::Symbol`, and a quoted symbol may contain spaces, parens or
/// quotes; `format!("({functor} {task})")` would render `"my task"` as the
/// two-argument literal `(completed my task)`. That silently changes the
/// predicate symbol, so an outstanding commitment would stop matching
/// (SPEC-006 REQ-601) and the emitted operand would name a literal the theory
/// never derives (SPEC-006 CON-601).
fn canonical(functor: &str, task: &str) -> String {
    Literal::new(
        functor,
        false,
        Mode::default(),
        Temporal::default(),
        vec![task.to_string()],
    )
    .to_spl()
}

/// The text argument of a documentation literal, when it is a symbol.
fn doc_text(c: &Conclusion) -> Option<&'static str> {
    match c.literal.predicate_args().get(1) {
        Some(Term::Symbol(s)) => Some(spindle_core::intern::resolve(*s)),
        _ => None,
    }
}

/// Envelope-derived provenance occupies the same `source` key a citation
/// does. Every claims-wrapped assert gets `source = agent:<did-tail>` from
/// spindle (SPEC-001 ADR-012), so an author who wrote no citation still has a
/// non-empty `source`. Treating that as provenance-not-citation is what keeps
/// SPEC-006 REQ-604's `missing-ready-source` gate reachable.
fn is_agent_provenance(s: &str) -> bool {
    s.starts_with(crate::core::closure::AGENT_ATOM_PREFIX)
}

fn meta_source(theory: &Theory, label: &str) -> Option<String> {
    match theory.get_meta(label)?.properties.get("source")? {
        MetaValue::String(s) if !s.trim().is_empty() && !is_agent_provenance(s) => Some(s.clone()),
        // A list-valued source is well-formed metadata but not a single
        // citation; treat it as absent rather than guessing which element.
        _ => None,
    }
}

/// True when an outstanding commitment's goal is `(completed task)`.
///
/// Reuses the canonical-form comparison the commitment ledger itself uses
/// (SPEC-006 REQ-601): both sides go through the one SPL recogniser and are
/// compared as canonical `to_spl()`. Never raw-text comparison.
fn promised(commitments: &[CommitmentState], goal_canon: &str) -> bool {
    commitments.iter().any(|c| {
        c.phase == CommitmentPhase::Outstanding
            && normalize_literal(&c.goal).is_some_and(|g| g == goal_canon)
    })
}

/// Project the selection object from one already-computed closure.
///
/// Single pass over `closure.conclusions`; the readiness rule label is read
/// off the conclusion the closure already recorded, so no literal is
/// re-derived (SPEC-006 NFR-601).
pub fn project(closure: &Closure) -> Selection {
    let mut tasks: BTreeSet<&str> = BTreeSet::new();
    // Template labels of every rule that derived `(ready ?x)`. A task can be
    // made ready by more than one rule; collecting them all and taking the
    // bytewise-least keeps the attribution independent of the order spindle
    // happens to emit conclusions in (SPEC-006 NFR-601). An entry with an
    // empty set is ready but has no recoverable rule.
    let mut ready: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut completed: BTreeSet<&str> = BTreeSet::new();
    // BTreeSet values give REQ-604's bytewise-least tie-break for free.
    let mut descriptions: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut acceptances: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

    for c in &closure.conclusions {
        if let Some(id) = match_arg0(c, P_TASK) {
            tasks.insert(id);
        } else if let Some(id) = match_arg0(c, P_READY) {
            let slot = ready.entry(id).or_default();
            if let Some(t) = c
                .rule_label
                .as_deref()
                .and_then(|l| resolve_template(&closure.theory, l))
            {
                slot.insert(t);
            }
        } else if let Some(id) = match_arg0(c, P_COMPLETED) {
            completed.insert(id);
        } else if let Some(id) = match_arg0(c, P_DESCRIPTION) {
            if let Some(t) = doc_text(c) {
                descriptions.entry(id).or_default().insert(t);
            }
        } else if let Some(id) = match_arg0(c, P_ACCEPTANCE) {
            if let Some(t) = doc_text(c) {
                acceptances.entry(id).or_default().insert(t);
            }
        }
    }

    let mut sel = Selection::default();
    // `tasks` is a BTreeSet, so iteration is already bytewise by task and both
    // output arrays inherit that order (SPEC-006 NFR-601).
    for task in tasks {
        let Some(rules) = ready.get(task) else {
            // Not positively ready: omitted from both arrays (CON-601).
            continue;
        };

        let mut withhold = |reason| {
            sel.withheld.push(WithheldItem {
                task: task.to_string(),
                reason,
            });
        };

        if completed.contains(task) {
            withhold(Reason::Completed);
            continue;
        }
        if promised(&closure.commitments, &canonical("completed", task)) {
            withhold(Reason::OutstandingCommitment);
            continue;
        }
        let Some(description) = descriptions.get(task).and_then(|s| s.iter().next()) else {
            withhold(Reason::MissingTaskDescription);
            continue;
        };
        let Some(acceptance) = acceptances.get(task).and_then(|s| s.iter().next()) else {
            withhold(Reason::MissingTaskAcceptance);
            continue;
        };
        // In practice spindle records one label per literal — a stronger
        // derivation takes the slot — so this set is normally a singleton.
        // Taking the bytewise-least keeps the choice deterministic if that
        // ever stops holding, without inventing a preference the reasoner
        // does not express.
        let Some(rule) = rules.iter().next().copied() else {
            withhold(Reason::MissingReadyRule);
            continue;
        };
        let Some(source) = meta_source(&closure.theory, rule) else {
            withhold(Reason::MissingReadySource);
            continue;
        };

        sel.next.push(Candidate {
            task: task.to_string(),
            description: (*description).to_string(),
            acceptance: (*acceptance).to_string(),
            ready_literal: canonical("ready", task),
            promise_goal: canonical("completed", task),
            ready_rule: rule.to_string(),
            source,
        });
    }
    sel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::closure::tests::Fixture;

    /// Standard fully-described ready task.
    fn base(f: &mut Fixture) {
        f.assert_spl(1, "(given (task models))");
        f.assert_spl(
            1,
            r#"(given (task-description models "Design the data model"))"#,
        );
        f.assert_spl(
            1,
            r#"(given (task-acceptance models "TEST-601 acceptance passes"))"#,
        );
        f.assert_spl(1, "(given (prerequisite-met models))");
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        f.assert_spl(
            1,
            r#"(meta r-ready (source "SPEC-006-elephant-next#TEST-601"))"#,
        );
    }

    const NOW: i64 = 1_784_000_000_000;

    fn project_of(f: &Fixture) -> Selection {
        project(&f.close("", NOW))
    }

    /// SPEC-006 TEST-601: render a fully described ready task.
    #[test]
    fn test_601_renders_described_ready_task() {
        let mut f = Fixture::new();
        base(&mut f);
        let s = project_of(&f);
        assert_eq!(s.withheld, vec![]);
        assert_eq!(
            s.next,
            vec![Candidate {
                task: "models".into(),
                description: "Design the data model".into(),
                acceptance: "TEST-601 acceptance passes".into(),
                ready_literal: "(ready models)".into(),
                promise_goal: "(completed models)".into(),
                ready_rule: "r-ready".into(),
                source: "SPEC-006-elephant-next#TEST-601".into(),
            }]
        );
    }

    /// SPEC-006 TEST-602: exclude completed and promised tasks.
    #[test]
    fn test_602_excludes_completed_and_promised() {
        let mut f = Fixture::new();
        base(&mut f);
        // A second and third task, both otherwise complete.
        for t in ["api", "ui"] {
            f.assert_spl(1, &format!("(given (task {t}))"));
            f.assert_spl(1, &format!(r#"(given (task-description {t} "desc"))"#));
            f.assert_spl(1, &format!(r#"(given (task-acceptance {t} "acc"))"#));
            f.assert_spl(1, &format!("(given (prerequisite-met {t}))"));
        }
        f.assert_spl(1, "(given (completed api))");
        f.commit(1, "", None, "(completed  ui)"); // double space: canonicalisation must normalise it

        let s = project_of(&f);
        assert_eq!(
            s.next.iter().map(|c| c.task.as_str()).collect::<Vec<_>>(),
            vec!["models"]
        );
        assert_eq!(
            s.withheld,
            vec![
                WithheldItem {
                    task: "api".into(),
                    reason: Reason::Completed
                },
                WithheldItem {
                    task: "ui".into(),
                    reason: Reason::OutstandingCommitment
                },
            ]
        );
    }

    /// SPEC-006 TEST-603: withhold a task missing work documentation.
    #[test]
    fn test_603_withholds_missing_task_documentation() {
        for (omit, want) in [
            ("task-description", Reason::MissingTaskDescription),
            ("task-acceptance", Reason::MissingTaskAcceptance),
        ] {
            let mut f = Fixture::new();
            base(&mut f);
            f.assert_spl(1, "(given (task cache))");
            f.assert_spl(1, "(given (prerequisite-met cache))");
            if omit != "task-description" {
                f.assert_spl(1, r#"(given (task-description cache "d"))"#);
            }
            if omit != "task-acceptance" {
                f.assert_spl(1, r#"(given (task-acceptance cache "a"))"#);
            }
            let s = project_of(&f);
            assert!(!s.next.iter().any(|c| c.task == "cache"), "omitting {omit}");
            assert_eq!(
                s.withheld,
                vec![WithheldItem {
                    task: "cache".into(),
                    reason: want
                }],
                "omitting {omit}"
            );
        }
    }

    /// SPEC-006 TEST-604: withhold a task missing readiness provenance.
    #[test]
    fn test_604_withholds_missing_ready_source() {
        let mut f = Fixture::new();
        // Readiness rule with no source metadata at all.
        f.assert_spl(1, "(given (task solo))");
        f.assert_spl(1, r#"(given (task-description solo "d"))"#);
        f.assert_spl(1, r#"(given (task-acceptance solo "a"))"#);
        f.assert_spl(1, "(given (prerequisite-met solo))");
        f.assert_spl(
            1,
            "(normally r-nosrc (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );

        let s = project_of(&f);
        assert_eq!(s.next, vec![]);
        assert_eq!(
            s.withheld,
            vec![WithheldItem {
                task: "solo".into(),
                reason: Reason::MissingReadySource
            }]
        );
    }

    /// SPEC-006 TEST-608 / REQ-606: idle is a successful, empty observation.
    #[test]
    fn test_608_idle_is_empty_not_error() {
        let f = Fixture::new();
        let s = project_of(&f);
        assert_eq!(s, Selection::default());
    }

    /// SPEC-006 TEST-609 / NFR-601: deterministic under entry reordering.
    #[test]
    fn test_609_deterministic_under_reordering() {
        let mut a = Fixture::new();
        base(&mut a);
        a.assert_spl(1, "(given (task zeta))");
        a.assert_spl(1, r#"(given (task-description zeta "z"))"#);
        a.assert_spl(1, r#"(given (task-acceptance zeta "z"))"#);
        a.assert_spl(1, "(given (prerequisite-met zeta))");

        // Same corpus, reversed authoring order.
        let mut b = Fixture::new();
        b.assert_spl(1, "(given (prerequisite-met zeta))");
        b.assert_spl(1, r#"(given (task-acceptance zeta "z"))"#);
        b.assert_spl(1, r#"(given (task-description zeta "z"))"#);
        b.assert_spl(1, "(given (task zeta))");
        base(&mut b);

        let (sa, sb) = (project_of(&a), project_of(&b));
        assert_eq!(sa, sb);
        assert_eq!(
            sa.next.iter().map(|c| c.task.as_str()).collect::<Vec<_>>(),
            vec!["models", "zeta"],
            "bytewise task order"
        );
    }

    /// SPEC-006 TEST-610: near-miss predicate shapes are ignored, not coerced.
    #[test]
    fn test_610_ignores_near_miss_shapes() {
        let mut f = Fixture::new();
        base(&mut f);
        f.assert_spl(1, "(given (task alpha beta))"); // wrong arity
        f.assert_spl(1, "(given (ready 42))"); // non-symbol argument
        f.assert_spl(1, "(given ready-legacy)"); // legacy flat atom
        f.assert_spl(1, "(given task-legacy)");

        let s = project_of(&f);
        assert_eq!(
            s.next.iter().map(|c| c.task.as_str()).collect::<Vec<_>>(),
            vec!["models"],
            "the valid task still projects"
        );
        assert_eq!(s.withheld, vec![], "near misses produce no withheld rows");
    }

    /// SPEC-006 TEST-611: report the template label, not the grounded instance,
    /// and attribute each task to the rule that actually derived its readiness.
    #[test]
    fn test_611_template_label_and_per_rule_attribution() {
        let mut f = Fixture::new();
        base(&mut f);
        // `hot` is ready only via the hotfix rule (no prerequisite-met).
        f.assert_spl(1, "(given (task hot))");
        f.assert_spl(1, r#"(given (task-description hot "h"))"#);
        f.assert_spl(1, r#"(given (task-acceptance hot "h"))"#);
        f.assert_spl(1, "(given (urgent hot))");
        f.assert_spl(
            1,
            "(normally r-ready-hotfix (and (task ?x) (urgent ?x)) (ready ?x))",
        );
        f.assert_spl(
            1,
            r#"(meta r-ready-hotfix (source "SPEC-006-elephant-next#TEST-611"))"#,
        );

        let s = project_of(&f);
        let by = |t: &str| s.next.iter().find(|c| c.task == t).cloned().unwrap();
        assert_eq!(by("models").ready_rule, "r-ready");
        assert_eq!(by("hot").ready_rule, "r-ready-hotfix");
        assert_eq!(by("hot").source, "SPEC-006-elephant-next#TEST-611");
        for c in &s.next {
            assert!(
                !c.ready_rule.contains('_'),
                "grounded instance leaked: {}",
                c.ready_rule
            );
        }
    }

    /// REQ-605: template recovery is existence-driven, not blind stripping.
    /// A rule genuinely named `phase_2` must survive intact — stripping it to
    /// `phase` would attribute readiness to a different rule (or to none).
    #[test]
    fn resolve_template_prefers_a_real_rule_over_stripping() {
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        f.assert_spl(
            1,
            "(normally phase_2 (and (task ?x) (urgent ?x)) (ready ?x))",
        );
        let th = &f.close("", NOW).theory;

        // A real rule whose own name ends in _<digits> is returned as-is.
        assert_eq!(resolve_template(th, "phase_2"), Some("phase_2"));
        // A grounded instance resolves to its template.
        assert_eq!(resolve_template(th, "r-ready_2"), Some("r-ready"));
        assert_eq!(resolve_template(th, "phase_2_7"), Some("phase_2"));
        // An exact template is returned unchanged.
        assert_eq!(resolve_template(th, "r-ready"), Some("r-ready"));
        // Nothing resolvable: the caller reports missing-ready-rule.
        assert_eq!(resolve_template(th, "nosuch_1"), None);
        assert_eq!(
            resolve_template(th, "r-ready_"),
            None,
            "empty tail not a run"
        );
    }

    /// CON-601's reason order, checked pairwise rather than only at the top:
    /// each adjacent pair must resolve to the earlier reason.
    #[test]
    fn withheld_reason_precedence_is_pairwise() {
        // 3 before 4: both documentation facts missing → description wins.
        let mut f = Fixture::new();
        base(&mut f);
        f.assert_spl(1, "(given (task bare))");
        f.assert_spl(1, "(given (prerequisite-met bare))");
        let w = &project_of(&f).withheld;
        assert_eq!(w[0].reason, Reason::MissingTaskDescription, "3 before 4");

        // 4 before 6: description present, acceptance absent, rule unsourced.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (task bare))");
        f.assert_spl(1, r#"(given (task-description bare "d"))"#);
        f.assert_spl(1, "(given (prerequisite-met bare))");
        f.assert_spl(
            1,
            "(normally r-nosrc (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        assert_eq!(
            project_of(&f).withheld[0].reason,
            Reason::MissingTaskAcceptance,
            "4 before 6"
        );

        // 2 before 3: promised and undocumented → commitment wins.
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (task bare))");
        f.assert_spl(1, "(given (prerequisite-met bare))");
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        f.commit(1, "", None, "(completed bare)");
        assert_eq!(
            project_of(&f).withheld[0].reason,
            Reason::OutstandingCommitment,
            "2 before 3"
        );
    }

    /// The ordered reason list of CON-601: the first applicable reason wins.
    #[test]
    fn withheld_reason_precedence_is_first_listed() {
        let mut f = Fixture::new();
        // completed *and* promised *and* undocumented — `completed` wins.
        f.assert_spl(1, "(given (task both))");
        f.assert_spl(1, "(given (prerequisite-met both))");
        f.assert_spl(1, "(given (completed both))");
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        f.commit(1, "", None, "(completed both)");
        let s = project_of(&f);
        assert_eq!(
            s.withheld,
            vec![WithheldItem {
                task: "both".into(),
                reason: Reason::Completed
            }]
        );
    }

    /// REQ-604: the claims wrapper auto-attaches `source = agent:<did>` to
    /// every rule label (SPEC-001 ADR-012). That provenance is not a citation,
    /// so it must not satisfy the gate — otherwise `missing-ready-source` is
    /// unreachable through the producer path and signer provenance is reported
    /// as if it were a specification reference.
    #[test]
    fn envelope_provenance_does_not_satisfy_the_source_gate() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (task solo))");
        f.assert_spl(1, r#"(given (task-description solo "d"))"#);
        f.assert_spl(1, r#"(given (task-acceptance solo "a"))"#);
        f.assert_spl(1, "(given (prerequisite-met solo))");
        f.assert_spl(
            1,
            "(normally r-nosrc (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        let c = f.close("", NOW);
        // The auto-attached provenance really is present and agent-shaped.
        let raw = c
            .theory
            .get_meta("r-nosrc")
            .and_then(|m| m.properties.get("source").cloned());
        assert!(
            matches!(&raw, Some(MetaValue::String(s)) if s.starts_with("agent:")),
            "expected claims-derived agent provenance, got {raw:?}"
        );
        // …and it is nonetheless treated as a missing citation.
        assert_eq!(
            project(&c).withheld,
            vec![WithheldItem {
                task: "solo".into(),
                reason: Reason::MissingReadySource
            }]
        );
    }

    /// REQ-601/CON-601: a task identifier that needs SPL quoting must still
    /// render as a one-argument literal. Interpolating it would produce
    /// `(completed my task)` — a `completed/2` literal — so the outstanding
    /// commitment would stop matching and the task would be offered anyway.
    #[test]
    fn quoted_identifier_still_matches_its_commitment() {
        let mut f = Fixture::new();
        f.assert_spl(1, r#"(given (task "my task"))"#);
        f.assert_spl(1, r#"(given (task-description "my task" "d"))"#);
        f.assert_spl(1, r#"(given (task-acceptance "my task" "a"))"#);
        f.assert_spl(1, r#"(given (prerequisite-met "my task"))"#);
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        f.assert_spl(1, r#"(meta r-ready (source "S"))"#);

        // Without the commitment the task is offered, with quoted operands.
        let s = project_of(&f);
        assert_eq!(s.next.len(), 1, "{s:?}");
        assert_eq!(s.next[0].ready_literal, r#"(ready "my task")"#);
        assert_eq!(s.next[0].promise_goal, r#"(completed "my task")"#);

        // With it, the task is withheld — the whole point of REQ-601.
        f.commit(1, "", None, r#"(completed "my task")"#);
        assert_eq!(
            project_of(&f).withheld,
            vec![WithheldItem {
                task: "my task".into(),
                reason: Reason::OutstandingCommitment
            }]
        );
    }

    /// REQ-601: `Mode` and `Temporal` sit alongside the name, so a deontic
    /// `(must (ready m))` reports name `ready` at arity 1. The theory does not
    /// derive `(ready m)`, so `m` must not be offered.
    #[test]
    fn modal_readiness_is_not_plain_readiness() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (task m))");
        f.assert_spl(1, r#"(given (task-description m "d"))"#);
        f.assert_spl(1, r#"(given (task-acceptance m "a"))"#);
        f.assert_spl(1, "(normally r-modal (task ?x) (must (ready ?x)))");
        f.assert_spl(1, r#"(meta r-modal (source "S"))"#);
        let s = project_of(&f);
        assert_eq!(s.next, vec![], "a modal readiness must not offer work");
        assert_eq!(s.withheld, vec![], "nor produce a withheld row");
    }

    /// REQ-605: `missing-ready-rule` — reachable when no prefix of the
    /// conclusion's label names a rule. A directly asserted `(given (ready X))`
    /// is labelled as a fact, which resolves, so this uses the resolver
    /// directly for the unresolvable case and checks the projection agrees.
    #[test]
    fn unresolvable_label_is_missing_ready_rule() {
        let mut f = Fixture::new();
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        let th = &f.close("", NOW).theory;
        assert_eq!(resolve_template(th, "ghost_9"), None);
        assert_eq!(resolve_template(th, "ghost"), None);
    }

    /// REQ-604: asserting readiness directly gives the conclusion a *fact*
    /// label carrying only auto-attached `agent:` provenance, and it subsumes
    /// the rule derivation — both the definite and defeasible conclusions come
    /// back labelled `f<n>`, never `r-ready`. The honest answer is therefore
    /// `missing-ready-source`: a bare fact is not a source-bearing readiness
    /// rule, which is exactly what REQ-604 requires a reviewer to have.
    #[test]
    fn directly_asserted_readiness_has_no_citation() {
        let mut f = Fixture::new();
        base(&mut f);
        f.assert_spl(1, "(given (ready models))");
        assert_eq!(
            project_of(&f).withheld,
            vec![WithheldItem {
                task: "models".into(),
                reason: Reason::MissingReadySource
            }]
        );
    }

    /// REQ-604/REQ-605: a *stronger* unsourced rule takes the conclusion's
    /// label. Here a definite `always a-ready` masks the defeasible, sourced
    /// `r-ready`, so the recorded derivation genuinely is the uncited one and
    /// the task is withheld. Documented because it is the behaviour an author
    /// will meet if they cite only the weaker rule.
    #[test]
    fn stronger_unsourced_rule_takes_the_label() {
        let mut f = Fixture::new();
        f.assert_spl(1, "(given (task models))");
        f.assert_spl(1, r#"(given (task-description models "d"))"#);
        f.assert_spl(1, r#"(given (task-acceptance models "a"))"#);
        f.assert_spl(1, "(given (prerequisite-met models))");
        f.assert_spl(1, "(always a-ready (task ?x) (ready ?x))");
        f.assert_spl(
            1,
            "(normally r-ready (and (task ?x) (prerequisite-met ?x)) (ready ?x))",
        );
        f.assert_spl(
            1,
            r#"(meta r-ready (source "SPEC-006-elephant-next#TEST-611"))"#,
        );

        assert_eq!(
            project_of(&f).withheld,
            vec![WithheldItem {
                task: "models".into(),
                reason: Reason::MissingReadySource
            }],
            "citing only the masked rule does not make the task offerable"
        );
    }

    /// REQ-604: duplicate documentation is an authoring wart, not an error;
    /// the bytewise-least text is chosen so NFR-601 stays satisfiable.
    #[test]
    fn duplicate_documentation_takes_bytewise_least() {
        let mut f = Fixture::new();
        base(&mut f);
        f.assert_spl(1, r#"(given (task-description models "AAA earlier"))"#);
        let s = project_of(&f);
        assert_eq!(s.next[0].description, "AAA earlier");
    }
}
