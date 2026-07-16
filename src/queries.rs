//! Consumer commands (SPEC-001 REQ-010..016; SPEC-003 REQ-208).
//! All queries are local: closure over the corpus, then pure function calls.

use crate::cli::Ctx;
use crate::core::closure::{self, Closure, CommitmentPhase};
use crate::errors::{AppError, AppResult};
use crate::store::{GENESIS_THEORY, TheoryStore};
use spindle_core::conclusion::ConclusionType;
use spindle_core::literal::Literal;

pub struct View {
    pub store: TheoryStore,
    pub closure: Closure,
    pub now_ms: i64,
}

/// Open the theory, run the closure at `--at` (or now).
pub fn view(ctx: &Ctx) -> AppResult<View> {
    let theory = ctx
        .theory
        .as_deref()
        .ok_or_else(|| AppError::Usage("no theory given: pass -t <theory> (id or alias)".into()))?;
    let mut store = TheoryStore::open(&ctx.paths, theory)?;
    // Apply pending MLS lane traffic (steward commits, rotated keybooks)
    // before reading: a member that missed a rotation could not otherwise
    // decrypt entries sealed under the new generation.
    if crate::e2ee::process_mls_lane(&ctx.paths, &store)? {
        store = TheoryStore::open(&ctx.paths, theory)?;
    }
    let now_ms = match &ctx.at {
        Some(ts) => chrono::DateTime::parse_from_rfc3339(ts)
            .map_err(|e| AppError::Parse(format!("--at is not RFC 3339: {e}")))?
            .timestamp_millis(),
        None => chrono::Utc::now().timestamp_millis(),
    };
    let (entries, _malformed) = store.entries();
    let trust = std::fs::read_to_string(ctx.paths.trust_file(&store.theory_id)).unwrap_or_default();
    let resolve = store.key_resolver();
    let closure = closure::close(
        &entries,
        &store.theory_id,
        GENESIS_THEORY,
        &resolve,
        &trust,
        now_ms,
    )?;
    Ok(View {
        store,
        closure,
        now_ms,
    })
}

/// Parse a user-typed literal through the single SPL recogniser.
pub fn parse_literal(text: &str) -> AppResult<Literal> {
    let t = spindle_parser::parse_spl(&format!("(given {})", text.trim()))
        .map_err(|e| AppError::Parse(format!("'{text}' is not a valid SPL literal: {e}")))?;
    t.rules()
        .next()
        .and_then(|r| r.head.first().cloned())
        .ok_or_else(|| AppError::Parse(format!("'{text}' did not parse to a literal")))
}

/// Render a literal for display and for pasting back into a query command.
/// `to_spl` already emits the single `(not …)` wrapper for a negated literal,
/// so a negated form is returned verbatim — the inner parens are load-bearing
/// (`not` takes exactly one argument, so `(not flies opus)` would be
/// rejected). A positive literal has its outer parens stripped for
/// readability; the flat-form sugar `(given <atom> <args…>)` still accepts the
/// result, so `elephant explain`/`why-not` round-trip it.
fn lit_display(l: &Literal) -> String {
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

// ── status (REQ-010) ────────────────────────────────────────────────────

pub fn status(ctx: &Ctx, trust: bool) -> AppResult<()> {
    let v = view(ctx)?;
    // One row per literal with its effective tag:
    // +D beats +d; any positive beats negatives; -D beats -d.
    fn rank(t: ConclusionType) -> u8 {
        match t {
            ConclusionType::DefinitelyProvable => 0,
            ConclusionType::DefeasiblyProvable => 1,
            ConclusionType::DefinitelyNotProvable => 2,
            ConclusionType::DefeasiblyNotProvable => 3,
        }
    }
    let mut best: std::collections::BTreeMap<String, ConclusionType> =
        std::collections::BTreeMap::new();
    let mut lits = std::collections::HashMap::new();
    for c in closure::presentable(&v.closure.conclusions) {
        let key = lit_display(&c.literal);
        lits.entry(key.clone()).or_insert_with(|| c.literal.clone());
        best.entry(key.clone())
            .and_modify(|t| {
                if rank(c.conclusion_type) < rank(*t) {
                    *t = c.conclusion_type;
                }
            })
            .or_insert(c.conclusion_type);
    }
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for (literal, tag) in &best {
        let mut row = serde_json::json!({
            "literal": literal,
            "tag": tag.symbol(),
        });
        if trust {
            if let Some(w) = v
                .closure
                .weighted
                .iter()
                .find(|w| Some(&w.literal) == lits.get(literal) && w.conclusion_type == *tag)
            {
                row["degree"] = serde_json::json!(w.degree);
                row["above_threshold"] = serde_json::json!(w.above_threshold);
            }
        }
        rows.push(row);
    }
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "conclusions": rows})
        );
    } else {
        if rows.is_empty() {
            println!("(empty theory)");
        }
        for r in &rows {
            let deg = r
                .get("degree")
                .and_then(|d| d.as_f64())
                .map(|d| format!("  (degree {d:.2})"))
                .unwrap_or_default();
            println!(
                "{:>3}  {}{}",
                r["tag"].as_str().unwrap(),
                r["literal"].as_str().unwrap(),
                deg
            );
        }
    }
    Ok(())
}

// ── explain (REQ-011) ───────────────────────────────────────────────────

pub fn explain(ctx: &Ctx, literal: &str) -> AppResult<()> {
    let v = view(ctx)?;
    let lit = parse_literal(literal)?;
    let expl = spindle_core::explanation::explain(&v.closure.theory, &lit)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    match expl {
        Some(e) => {
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({"v":1, "theory": v.store.theory_id,
                        "literal": literal, "explanation": e.to_json()})
                );
            } else {
                println!("{}", e.to_natural_language());
            }
        }
        None => {
            // A bare `"explanation": null` is indistinguishable from a
            // serialization failure. Say why it is null (the literal is not
            // provable, so there is no derivation to explain) and point at the
            // command that does explain the block (SPEC-001 REQ-011/REQ-012).
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({"v":1, "theory": v.store.theory_id,
                        "literal": literal, "explanation": null,
                        "not_provable": true,
                        "hint": format!("see `elephant why-not {literal}`")})
                );
            } else {
                println!("{literal} is not provable — try `elephant why-not {literal}`");
            }
        }
    }
    Ok(())
}

// ── why-not (REQ-012; SPEC-005 REQ-405 docs join) ───────────────────────

/// SPEC-005 REQ-405: the docs object for every *documented* family among
/// the given literals — corpus documentation and built-in registry alike.
/// Undocumented families are omitted, so output stays byte-identical to
/// the pre-405 shape when nothing is documented. `documenter`/`redefined`
/// deliberately omitted (provenance lives in the vocab view).
fn docs_join<'a>(
    vv: &crate::core::vocab::VocabView,
    lits: impl Iterator<Item = &'a Literal>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    // Process legacy before predicate so that in the (pathological) case
    // of a flat atom spelled like an indicator sharing a rendered key with
    // a documented predicate family, the predicate entry wins
    // deterministically; the family_kind field carries the discriminant.
    let mut fams: Vec<crate::core::vocab::Family> = lits
        .map(|l| crate::core::vocab::family(l, &vv.tasks))
        .collect();
    fams.sort();
    fams.reverse();
    fams.dedup();
    for fam in fams {
        let row = vv.rows.iter().find(|r| r.family == fam);
        let Some(row) = row else { continue };
        let Some(desc) = &row.doc.description else {
            continue;
        };
        let mut o = serde_json::Map::new();
        o.insert("family_kind".into(), fam.kind().into());
        o.insert("description".into(), desc.clone().into());
        if let Some(k) = &row.doc.kind {
            o.insert("kind".into(), k.clone().into());
        }
        if let Some(a) = &row.doc.asserter {
            o.insert("asserter".into(), a.clone().into());
        }
        o.insert("built_in".into(), row.built_in.into());
        out.insert(fam.rendered(), o.into());
    }
    out
}

/// The description a literal's family carries, if documented (text mode).
fn doc_of<'a>(vv: &'a crate::core::vocab::VocabView, l: &Literal) -> Option<&'a str> {
    let fam = crate::core::vocab::family(l, &vv.tasks);
    vv.rows
        .iter()
        .find(|r| r.family == fam)
        .and_then(|r| r.doc.description.as_deref())
}

/// Ambiguity-blocking analysis for an `undetermined` why-not blocker (#12).
///
/// Spindle reports `undetermined` when a rule's body is satisfied, no attacker
/// has a satisfied body, yet the literal is still not proven — the honest
/// signature of *ambiguity blocking*: the head and its complement are each
/// supported and mutually block, with no preference to break the tie, so both
/// stay `-D`. Return the opposing literal (the complement) and the rule
/// label(s) concluding it, so the intended-honest state of an un-adjudicated
/// conflict gets a precise explanation rather than a "diagnostics gap"
/// fallthrough. `None` when no opposer concludes the complement (a genuine
/// diagnostics gap, which stays `undetermined`).
fn ambiguity_opposers(closure: &Closure, rule_label: &str) -> Option<(Literal, Vec<String>)> {
    let theory = &closure.theory;
    let complement = theory.get_rule(rule_label)?.head_literal().complement();
    let comp_spl = complement.to_spl();

    // Cite only *applicable* opposers. Spindle's `undetermined` is ambiguous
    // between a genuine mutual block and a diagnostics gap, so a rule whose
    // head merely matches the complement is not enough: if any of its body
    // literals has no support at all (not positively concluded, not the head
    // of any rule/fact) the opposer is inert and citing it would assert a
    // mutual block that does not exist. A body literal that IS concluded or
    // IS some rule's head has live — if itself contested — support, which is
    // exactly the standing of a real ambiguity participant.
    let positively_concluded: std::collections::HashSet<String> = closure
        .conclusions
        .iter()
        .filter(|c| c.conclusion_type.is_positive())
        .map(|c| c.literal.to_spl())
        .collect();
    let rule_heads: std::collections::HashSet<String> =
        theory.rules().map(|r| r.head_literal().to_spl()).collect();
    let has_support =
        |spl: &str| positively_concluded.contains(spl) || rule_heads.contains(spl);

    let mut opposers: Vec<String> = theory
        .rules()
        .filter(|r| r.label != rule_label && r.head_literal().to_spl() == comp_spl)
        .filter(|r| {
            r.body
                .iter()
                .filter_map(|bl| bl.as_logic().map(|l| l.to_literal()))
                .all(|l| has_support(&l.to_spl()))
        })
        .map(|r| r.template_label().to_string())
        .collect();
    opposers.sort();
    opposers.dedup();
    (!opposers.is_empty()).then_some((complement, opposers))
}

pub fn why_not(ctx: &Ctx, literal: &str) -> AppResult<()> {
    use spindle_core::query::BlockingType;
    let v = view(ctx)?;
    let lit = parse_literal(literal)?;
    let r = spindle_core::query::why_not(&v.closure.theory, &lit)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    let vv = crate::core::vocab::view(&v.closure);
    if ctx.json {
        let blocked: Vec<_> = r
            .blocked_by
            .iter()
            .map(|b| {
                let mut o = serde_json::json!({
                    "type": b.blocking_type.to_string(),
                    "rule": b.rule_label,
                    "missing": b.missing_literals.iter().map(lit_display).collect::<Vec<_>>(),
                    "blocking_rule": b.blocking_rule,
                    "explanation": b.explanation,
                });
                if b.blocking_type == BlockingType::Undetermined
                    && let Some((opp, opposers)) = ambiguity_opposers(&v.closure, &b.rule_label)
                {
                    let head = v
                        .closure
                        .theory
                        .get_rule(&b.rule_label)
                        .map(|r| lit_display(r.head_literal()))
                        .unwrap_or_else(|| literal.to_string());
                    let opp_disp = lit_display(&opp);
                    o["type"] = "ambiguity".into();
                    o["opposing_literal"] = opp_disp.clone().into();
                    o["opposing_rules"] = opposers.clone().into();
                    o["explanation"] = format!(
                        "ambiguity blocking: {head} and {opp_disp} are each supported \
                         (the latter by {}) and mutually block with no preference to break \
                         the tie, so both stay -D; assert a `(prefer …)` to adjudicate",
                        opposers.join(", ")
                    )
                    .into();
                }
                o
            })
            .collect();
        let mut obj = serde_json::json!({"v":1, "theory": v.store.theory_id, "literal": literal,
            "would_derive": r.would_derive, "blocked_by": blocked});
        let docs = docs_join(
            &vv,
            r.blocked_by.iter().flat_map(|b| b.missing_literals.iter()),
        );
        if !docs.is_empty() {
            obj["docs"] = docs.into();
        }
        println!("{obj}");
    } else {
        if r.blocked_by.is_empty() {
            println!("no rule concludes {literal} — nothing to block");
        }
        for b in &r.blocked_by {
            // Upgrade the diagnostics-gap fallback to a named ambiguity block.
            let ambiguity = (b.blocking_type == BlockingType::Undetermined)
                .then(|| ambiguity_opposers(&v.closure, &b.rule_label))
                .flatten();
            match &ambiguity {
                Some((opp, opposers)) => println!(
                    "rule {}: ambiguity blocking — {} is equally supported by {} with no \
                     preference to break the tie (assert a `(prefer …)` to adjudicate)",
                    b.rule_label,
                    crate::core::vocab::escape_controls(&lit_display(opp)),
                    opposers.join(", "),
                ),
                None => println!("rule {}: {}", b.rule_label, b.explanation),
            }
            for m in &b.missing_literals {
                if let Some(desc) = doc_of(&vv, m) {
                    println!(
                        "  {} — {}",
                        crate::core::vocab::escape_controls(&lit_display(m)),
                        crate::core::vocab::escape_controls(desc)
                    );
                }
            }
        }
    }
    Ok(())
}

// ── require / abduction (REQ-013) ───────────────────────────────────────

pub fn require(ctx: &Ctx, literal: &str) -> AppResult<()> {
    use spindle_core::query::{
        DEFAULT_MAX_RAW_CANDIDATES, RequiresOptions, RequiresSearchStatus, requires_with_options,
    };
    let v = view(ctx)?;
    let lit = parse_literal(literal)?;
    // Verified abduction (REQ-013, #16): `requires_with_options` injects each
    // raw candidate and re-reasons, keeping only fact sets that make the goal
    // positively provable under the same semantics as `what-if`. Raw `abduce`
    // returned body-satisfaction candidates that an applicable defeater or a
    // competing rule could still block — a remedy the operator would assert
    // in vain. `search_status` records whether the search was exhaustive.
    // Cap on solutions returned. Spindle reports `BoundedComplete` as soon as
    // this many are accepted, even if more verified solutions remain, so hitting
    // the cap is treated as non-exhaustive below rather than claiming the search
    // was complete (#16 review).
    const MAX_SOLUTIONS: usize = 8;
    let r = requires_with_options(
        &v.closure.theory,
        &lit,
        RequiresOptions {
            max_solutions: MAX_SOLUTIONS,
            max_raw_candidates: DEFAULT_MAX_RAW_CANDIDATES,
        },
    )
    .map_err(|e| AppError::Reasoner(e.to_string()))?;
    let vv = crate::core::vocab::view(&v.closure);
    let already = r.already_provable;
    // Every returned solution is already verified and open (an already-provable
    // goal yields no solutions), so no client-side filtering is needed.
    let open: Vec<&spindle_core::query::AbductionSolution> = r.solutions.iter().collect();
    let solutions: Vec<Vec<String>> = open
        .iter()
        .map(|s| {
            let mut f: Vec<String> = s.facts.iter().map(lit_display).collect();
            f.sort();
            f
        })
        .collect();
    // Exhaustive only if the search terminated within bounds AND the returned
    // count did not hit the solution cap (spindle reports BoundedComplete on
    // reaching `max_solutions`, which is not the same as "no more exist").
    let exhaustive = matches!(r.search_status, RequiresSearchStatus::BoundedComplete)
        && solutions.len() < MAX_SOLUTIONS;
    if ctx.json {
        let mut obj = serde_json::json!({"v":1, "theory": v.store.theory_id, "goal": literal,
            "already_provable": already, "solutions": solutions,
            "search_status": if exhaustive { "bounded-complete" } else { "budget-exhausted" },
            "verification": {
                "raw_examined": r.verification.raw_examined,
                "accepted": r.verification.accepted,
                "rejected": r.verification.rejected,
            }});
        let docs = docs_join(&vv, open.iter().flat_map(|s| s.facts.iter()));
        if !docs.is_empty() {
            obj["docs"] = docs.into();
        }
        println!("{obj}");
    } else if already {
        println!("{literal} is already provable");
    } else if solutions.is_empty() {
        println!(
            "no fact set found that would prove {literal} ({})",
            if exhaustive {
                "bounded search, exhausted"
            } else {
                "search budget reached"
            }
        );
    } else {
        println!("provable if all added:");
        for s in &open {
            let rendered: Vec<String> = {
                let mut f: Vec<String> = s
                    .facts
                    .iter()
                    .map(|l| match doc_of(&vv, l) {
                        Some(d) => format!(
                            "{} — {}",
                            crate::core::vocab::escape_controls(&lit_display(l)),
                            crate::core::vocab::escape_controls(d)
                        ),
                        None => lit_display(l),
                    })
                    .collect();
                f.sort();
                f
            };
            println!("  {}", rendered.join("  "));
        }
        if !exhaustive {
            println!("(search budget reached — more solutions may exist)");
        }
    }
    Ok(())
}

// ── what-if (REQ-014) ───────────────────────────────────────────────────

/// A what-if hypothetical must be a *fact*. what-if adds each hypothetical to
/// the theory as a fact and re-reasons — spindle's `HypotheticalClaim` is
/// fact-only — so a `(prefer …)` or a rule handed to it is coerced into an
/// inert atom by the `(given …)` sugar and never installed as a superiority or
/// rule. That silently produced a wrong answer for the single most useful
/// hypothetical (an *adjudication*), so reject it with a clear error instead
/// (#11). Detection reuses the SPL recogniser: a plain fact literal is not a
/// standalone statement (it must be wrapped `(given …)`), so a raw string that
/// parses to a superiority or a non-fact rule is structure what-if cannot take.
fn structural_hypothetical(raw: &str) -> Option<&'static str> {
    let t = spindle_parser::parse_spl(raw.trim()).ok()?;
    if !t.superiorities().is_empty() {
        return Some("a preference (prefer …)");
    }
    if t.rules().any(|r| !r.is_fact()) {
        return Some("a rule");
    }
    None
}

pub fn what_if(ctx: &Ctx, facts_then_goal: &[String]) -> AppResult<()> {
    let (goal_text, facts) = facts_then_goal
        .split_last()
        .ok_or_else(|| AppError::Usage("what-if needs <facts…> <goal>".into()))?;
    let v = view(ctx)?;
    let goal = parse_literal(goal_text)?;
    for f in facts {
        if let Some(kind) = structural_hypothetical(f) {
            return Err(AppError::Usage(format!(
                "what-if hypotheticals must be facts, but '{f}' is {kind}, which \
                 what-if cannot install — it adds each hypothetical as a fact and \
                 re-reasons. To test an adjudication non-destructively is not yet \
                 supported here; assert it and then retract it: \
                 `elephant assert '{f}'` followed by `elephant retract <sentence-id>`."
            )));
        }
    }
    let hyps: Vec<spindle_core::query::HypotheticalClaim> = facts
        .iter()
        .map(|f| {
            Ok(spindle_core::query::HypotheticalClaim::new(parse_literal(
                f,
            )?))
        })
        .collect::<AppResult<_>>()?;
    let r = spindle_core::query::what_if(&v.closure.theory, hyps, &goal)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "goal": goal_text,
                "provable": r.is_provable(),
                "newly_provable": r.newly_provable().iter().map(lit_display).collect::<Vec<_>>(),
                "changed": r.changed_conclusions.iter()
                    .map(|(l, from, to)| serde_json::json!({
                        "literal": lit_display(l),
                        "from": from.symbol(), "to": to.symbol()}))
                    .collect::<Vec<_>>()})
        );
    } else {
        println!(
            "{}  {}",
            if r.is_provable() { "+d" } else { "-d" },
            goal_text
        );
        for l in r.newly_provable() {
            println!("now provable: {}", lit_display(l));
        }
    }
    Ok(())
}

// ── commitments (REQ-015) ───────────────────────────────────────────────

pub fn commitments(ctx: &Ctx) -> AppResult<()> {
    let v = view(ctx)?;
    if ctx.json {
        let items: Vec<_> = v
            .closure
            .commitments
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id, "by": c.by_agent, "trigger": c.trigger,
                    "deadline": c.deadline, "goal": c.goal,
                    "state": c.phase.to_string(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "commitments": items})
        );
    } else if v.closure.commitments.is_empty() {
        println!("no commitments — `elephant promise <goal>` makes one");
    } else {
        for c in &v.closure.commitments {
            let dl = c
                .deadline
                .as_deref()
                .map(|d| format!(" by {d}"))
                .unwrap_or_default();
            let trig = if c.trigger.is_empty() {
                String::new()
            } else {
                format!(" when {}", c.trigger)
            };
            println!(
                "{:<12} {}  {} → {}{}{}",
                c.phase.to_string(),
                c.id,
                closure::source_atom(&c.by_agent),
                c.goal,
                trig,
                dl
            );
        }
        // Violations are the paper's point: surface them loudly.
        let violated = v
            .closure
            .commitments
            .iter()
            .filter(|c| c.phase == CommitmentPhase::Violated)
            .count();
        if violated > 0 {
            println!("({violated} violated — an elephant never forgets)");
        }
    }
    Ok(())
}

// ── show: inspect one entry by sentence-id (#9) ─────────────────────────

/// The read counterpart to the sentence-ids that producers mint: locate the
/// single admitted entry the id names and print it, rather than forcing a
/// `log --json | jq 'select(.sentence_id==…)'` round-trip. Reuses the same
/// by-id lookup and the same "no entry with sentence-id …" miss error the
/// retract/concede pre-checks use (src/cli.rs).
pub fn show(ctx: &Ctx, sentence_id: &str) -> AppResult<()> {
    use crate::core::envelope::SpeechAct;
    let v = view(ctx)?;
    // Resolve by the act's own sentence-id, or — for retracts/concedes, which
    // carry none — by the stable `entry_id` that `log` advertises as their
    // addressable id (#14). Without the second arm, `show` could not inspect
    // the very ids `log` hands out for those entries.
    let a = v
        .closure
        .admitted
        .iter()
        .find(|a| {
            a.sid.as_deref() == Some(sentence_id)
                || crate::core::envelope::Entry::sentence_id(
                    &a.entry.theory,
                    &a.entry.signer,
                    a.entry.hlc,
                ) == sentence_id
        })
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "no entry with sentence-id {sentence_id} in this theory"
            ))
        })?;

    let status = if a.retracted {
        "retracted"
    } else if a.label_shadowed {
        "shadowed"
    } else {
        "active"
    };
    let timestamp = chrono::DateTime::from_timestamp_millis(a.entry.hlc.wall_ms as i64)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default();

    // Act-specific projection: the SPL/literal form, plus the two back-refs a
    // speech act can carry (concede → in_reply_to, retract → retracts).
    let (spl_form, in_reply_to, retracts): (Option<String>, Option<String>, Option<String>) =
        match &a.act {
            SpeechAct::Assert { spl, .. } => (Some(spl.clone()), None, None),
            SpeechAct::Commit {
                goal, trigger, by, ..
            } => {
                let mut s = format!("commit {goal}");
                if !trigger.is_empty() {
                    s.push_str(&format!(" when {trigger}"));
                }
                if let Some(b) = by {
                    s.push_str(&format!(" by {b}"));
                }
                (Some(s), None, None)
            }
            SpeechAct::Request {
                goal,
                addressee,
                trigger,
                ..
            } => {
                let mut s = format!("request {addressee} {goal}");
                if !trigger.is_empty() {
                    s.push_str(&format!(" when {trigger}"));
                }
                (Some(s), None, None)
            }
            SpeechAct::Retract { target, reason } => {
                let s = if reason.is_empty() {
                    "retract".into()
                } else {
                    format!("retract ({reason})")
                };
                (Some(s), None, Some(target.clone()))
            }
            SpeechAct::Concede {
                literal,
                in_reply_to: irt,
            } => (Some(literal.clone()), Some(irt.clone()), None),
            other => (Some(other.performative().to_string()), None, None),
        };

    if ctx.json {
        println!(
            "{}",
            serde_json::json!({
                "v": 1,
                "theory": v.store.theory_id,
                "sentence_id": sentence_id,
                "performative": a.act.performative(),
                "signer": a.entry.signer,
                "hlc": {"wall_ms": a.entry.hlc.wall_ms, "logical": a.entry.hlc.logical},
                "timestamp": timestamp,
                "status": status,
                "spl_form": spl_form,
                "in_reply_to": in_reply_to,
                "retracts": retracts,
                "entry": serde_json::to_value(&a.entry).unwrap_or(serde_json::Value::Null),
            })
        );
    } else {
        println!("{}  {sentence_id}", a.act.performative());
        if let Some(s) = &spl_form {
            println!("  {}", crate::core::vocab::escape_controls(s));
        }
        println!("  signer   {}", a.entry.signer);
        println!("  hlc      {timestamp}  (logical {})", a.entry.hlc.logical);
        if let Some(r) = &in_reply_to {
            println!("  re       {r}");
        }
        if let Some(t) = &retracts {
            println!("  retracts {t}");
        }
        println!("  status   {status}");
    }
    Ok(())
}

// ── log / journal (REQ-016) ─────────────────────────────────────────────

pub fn log(ctx: &Ctx) -> AppResult<()> {
    let v = view(ctx)?;
    // SPEC-005 REQ-407: annotate Entries that overwrite another signer's
    // winning documentation value (family, key, previous writer).
    let redefs = crate::core::vocab::redefinitions(&v.closure);
    for r in &redefs {
        // OBS-402: redefinition events logged at journal time.
        tracing::info!(
            family = %r.family.rendered(),
            key = %r.key,
            previous = %r.previous_writer,
            entry = %r.sid,
            "vocabulary documentation redefined"
        );
    }
    let mut items: Vec<serde_json::Value> = Vec::new();
    for a in &v.closure.admitted {
        // Every entry gets a stable, individually-addressable `entry_id`
        // derived from the entry's own (theory, signer, hlc) — the exact
        // derivation the producer used for a sentence-id, so for an assert it
        // equals `sid` (including the genesis entry, whose id is derived under
        // the sentinel `genesis` theory, not the store id). Retracts (and
        // concedes) carry no `sid` of their own, so without this an
        // order-independent journal fingerprint keyed on `sid` silently
        // collapses every retract onto `null` (#14).
        let entry_id = crate::core::envelope::Entry::sentence_id(
            &a.entry.theory,
            &a.entry.signer,
            a.entry.hlc,
        );
        let mut item = serde_json::json!({
            "sid": a.sid,
            "entry_id": entry_id,
            "signer": a.entry.signer,
            "performative": a.act.performative(),
            "hlc": {"wall_ms": a.entry.hlc.wall_ms, "logical": a.entry.hlc.logical},
            "status": if a.retracted {
                "retracted"
            } else if a.label_shadowed {
                "shadowed"
            } else {
                "active"
            },
            "cbcl": a.entry.cbcl,
        });
        // A retract's target was only reachable inside the opaque `cbcl`
        // blob; surface it as a first-class field so retracts are addressable.
        if let crate::core::envelope::SpeechAct::Retract { target, .. } = &a.act {
            item["retracts"] = serde_json::json!(target);
        }
        let mine: Vec<serde_json::Value> = redefs
            .iter()
            .filter(|r| Some(&r.sid) == a.sid.as_ref())
            .map(|r| {
                serde_json::json!({
                    "family": r.family.rendered(),
                    "family_kind": r.family.kind(),
                    "key": r.key,
                    "previous_writer": r.previous_writer,
                })
            })
            .collect();
        if !mine.is_empty() {
            item["redefines"] = mine.into();
        }
        items.push(item);
    }
    for (e, q) in &v.closure.quarantined {
        let entry_id = crate::core::envelope::Entry::sentence_id(&e.theory, &e.signer, e.hlc);
        items.push(serde_json::json!({
            "sid": null,
            "entry_id": entry_id,
            "signer": e.signer,
            "performative": null,
            "hlc": {"wall_ms": e.hlc.wall_ms, "logical": e.hlc.logical},
            "status": format!("quarantined: {q}"),
            "cbcl": e.cbcl,
        }));
    }
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "entries": items})
        );
    } else {
        for i in &items {
            let redef = i["redefines"]
                .as_array()
                .map(|rs| {
                    rs.iter()
                        .map(|r| {
                            format!(
                                "  [redefines {} {} — previously by {}]",
                                crate::core::vocab::escape_controls(
                                    r["family"].as_str().unwrap_or("?")
                                ),
                                r["key"].as_str().unwrap_or("?"),
                                r["previous_writer"].as_str().unwrap_or("?"),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            // Identify by the act's own sid where it has one, else the stable
            // entry_id (retracts/concedes) so every line is addressable.
            let id = i["sid"]
                .as_str()
                .or_else(|| i["entry_id"].as_str())
                .unwrap_or("-");
            let retracts = i["retracts"]
                .as_str()
                .map(|t| format!(" → {t}"))
                .unwrap_or_default();
            println!(
                "{}  {:<9} {:<8} {}{}{}",
                i["hlc"]["wall_ms"],
                i["performative"].as_str().unwrap_or("?"),
                i["status"].as_str().unwrap_or("?"),
                id,
                retracts,
                redef,
            );
        }
    }
    Ok(())
}

// ── describe / trace (SPEC-003 REQ-208) ─────────────────────────────────

/// A meta property value as plain JSON — a string stays a string, a list
/// becomes a JSON array. The `--json` contract (SPEC-003 REQ-208) is a stable
/// data interface: rendering `MetaValue` through `Debug` (`format!("{val:?}")`)
/// leaked Rust syntax (`String("…")`) that every consumer then had to strip.
fn meta_value_json(val: &spindle_core::theory::MetaValue) -> serde_json::Value {
    use spindle_core::theory::MetaValue;
    match val {
        MetaValue::String(s) => serde_json::Value::String(s.clone()),
        MetaValue::List(items) => {
            serde_json::Value::Array(items.iter().cloned().map(serde_json::Value::String).collect())
        }
    }
}

pub fn describe(ctx: &Ctx, labels: &[String]) -> AppResult<()> {
    let v = view(ctx)?;
    let mut out = Vec::new();
    for label in labels {
        let rule = v.closure.theory.get_rule(label);
        let meta = v.closure.theory.get_meta(label);
        let props: serde_json::Value = match meta {
            Some(m) => serde_json::Value::Object(
                m.properties
                    .iter()
                    .map(|(k, val)| (k.clone(), meta_value_json(val)))
                    .collect(),
            ),
            None => serde_json::json!({}),
        };
        out.push(serde_json::json!({
            "label": label,
            "rule": rule.map(|r| r.to_spl()),
            "meta": props,
        }));
        if !ctx.json {
            match rule {
                Some(r) => println!("{label}\n  {}", r.to_spl()),
                None => println!("{label}\n  (no such rule)"),
            }
        }
    }
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "labels": out})
        );
    }
    Ok(())
}

// ── vocab (SPEC-005 REQ-401) ────────────────────────────────────────────

pub fn vocab(ctx: &Ctx) -> AppResult<()> {
    use crate::core::vocab::escape_controls;
    let v = view(ctx)?;
    let vv = crate::core::vocab::view(&v.closure);
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id,
                "vocab": crate::core::vocab::view_json(&vv)})
        );
        return Ok(());
    }
    if vv.rows.is_empty() {
        println!("(no vocabulary — empty theory)");
        return Ok(());
    }
    for r in &vv.rows {
        let roles: Vec<&str> = r.roles.iter().map(|x| x.name()).collect();
        let mut markers: Vec<String> = Vec::new();
        if r.built_in {
            markers.push("built-in".into());
        }
        if r.doc.detached {
            markers.push("detached".into());
        }
        if r.doc.redefined {
            markers.push("redefined".into());
        }
        for k in &r.doc.malformed {
            markers.push(format!("malformed:{k}"));
        }
        let marker_txt = if markers.is_empty() {
            String::new()
        } else {
            format!("  [{}]", markers.join(" "))
        };
        let desc = r
            .doc
            .description
            .as_deref()
            .map(|d| format!("  — {}", escape_controls(d)))
            .unwrap_or_default();
        println!(
            "{:<7} {:<10} {:<28} {}{}{}",
            r.class.name(),
            r.family.kind(),
            escape_controls(&r.family.rendered()),
            roles.join(","),
            desc,
            marker_txt,
        );
    }
    Ok(())
}

pub fn trace(ctx: &Ctx) -> AppResult<()> {
    let v = view(ctx)?;
    if ctx.json {
        let rows: Vec<_> = v
            .closure
            .conclusions
            .iter()
            .map(|c| {
                serde_json::json!({
                    "literal": lit_display(&c.literal),
                    "tag": c.conclusion_type.symbol(),
                    "rule": c.rule_label,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "trace": rows})
        );
    } else {
        for c in &v.closure.conclusions {
            println!(
                "{:>3}  {:<40} {}",
                c.conclusion_type.symbol(),
                lit_display(&c.literal),
                c.rule_label.as_deref().unwrap_or("")
            );
        }
    }
    Ok(())
}

// Positive conclusions as (display, type) pairs — used by p2p membership
// derivation and the join ceremony.
pub fn conclusions_positive(v: &View) -> Vec<(String, ConclusionType)> {
    closure::presentable(&v.closure.conclusions)
        .filter(|c| c.conclusion_type.is_positive() && !c.literal.negation)
        .map(|c| (lit_display(&c.literal), c.conclusion_type))
        .collect()
}
