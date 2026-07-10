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

fn lit_display(l: &Literal) -> String {
    let inner = l.to_spl();
    let stripped = inner
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&inner);
    if l.negation {
        format!("(not {stripped})")
    } else {
        stripped.to_string()
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
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({"v":1, "theory": v.store.theory_id,
                        "literal": literal, "explanation": null})
                );
            } else {
                println!("{literal} is not provable — try `elephant why-not {literal}`");
            }
        }
    }
    Ok(())
}

// ── why-not (REQ-012) ───────────────────────────────────────────────────

pub fn why_not(ctx: &Ctx, literal: &str) -> AppResult<()> {
    let v = view(ctx)?;
    let lit = parse_literal(literal)?;
    let r = spindle_core::query::why_not(&v.closure.theory, &lit)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    if ctx.json {
        let blocked: Vec<_> = r
            .blocked_by
            .iter()
            .map(|b| {
                serde_json::json!({
                    "type": b.blocking_type.to_string(),
                    "rule": b.rule_label,
                    "missing": b.missing_literals.iter().map(lit_display).collect::<Vec<_>>(),
                    "blocking_rule": b.blocking_rule,
                    "explanation": b.explanation,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "literal": literal,
                "would_derive": r.would_derive, "blocked_by": blocked})
        );
    } else {
        if r.blocked_by.is_empty() {
            println!("no rule concludes {literal} — nothing to block");
        }
        for b in &r.blocked_by {
            println!("rule {}: {}", b.rule_label, b.explanation);
        }
    }
    Ok(())
}

// ── require / abduction (REQ-013) ───────────────────────────────────────

pub fn require(ctx: &Ctx, literal: &str) -> AppResult<()> {
    let v = view(ctx)?;
    let lit = parse_literal(literal)?;
    let r = spindle_core::query::abduce(&v.closure.theory, &lit, 8)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    let already = r.solutions.iter().any(|s| s.is_already_provable());
    let solutions: Vec<Vec<String>> = r
        .solutions
        .iter()
        .filter(|s| !s.is_already_provable())
        .map(|s| {
            let mut f: Vec<String> = s.facts.iter().map(lit_display).collect();
            f.sort();
            f
        })
        .collect();
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": v.store.theory_id, "goal": literal,
                "already_provable": already, "solutions": solutions})
        );
    } else if already {
        println!("{literal} is already provable");
    } else if solutions.is_empty() {
        println!("no fact set found that would prove {literal} (bounded search)");
    } else {
        println!("provable if all added:");
        for s in &solutions {
            println!("  {}", s.join("  "));
        }
    }
    Ok(())
}

// ── what-if (REQ-014) ───────────────────────────────────────────────────

pub fn what_if(ctx: &Ctx, facts_then_goal: &[String]) -> AppResult<()> {
    let (goal_text, facts) = facts_then_goal
        .split_last()
        .ok_or_else(|| AppError::Usage("what-if needs <facts…> <goal>".into()))?;
    let v = view(ctx)?;
    let goal = parse_literal(goal_text)?;
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

// ── log / journal (REQ-016) ─────────────────────────────────────────────

pub fn log(ctx: &Ctx) -> AppResult<()> {
    let v = view(ctx)?;
    let mut items: Vec<serde_json::Value> = Vec::new();
    for a in &v.closure.admitted {
        items.push(serde_json::json!({
            "sid": a.sid,
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
        }));
    }
    for (e, q) in &v.closure.quarantined {
        items.push(serde_json::json!({
            "sid": null,
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
            println!(
                "{}  {:<9} {:<8} {}",
                i["hlc"]["wall_ms"],
                i["performative"].as_str().unwrap_or("?"),
                i["status"].as_str().unwrap_or("?"),
                i["sid"].as_str().unwrap_or("-"),
            );
        }
    }
    Ok(())
}

// ── describe / trace (SPEC-003 REQ-208) ─────────────────────────────────

pub fn describe(ctx: &Ctx, labels: &[String]) -> AppResult<()> {
    let v = view(ctx)?;
    let mut out = Vec::new();
    for label in labels {
        let rule = v.closure.theory.get_rule(label);
        let meta = v.closure.theory.get_meta(label);
        let props: serde_json::Value = match meta {
            Some(m) => serde_json::json!(
                m.properties
                    .iter()
                    .map(|(k, val)| (k.clone(), format!("{val:?}")))
                    .collect::<std::collections::BTreeMap<_, _>>()
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

// Re-export for board/next (SPEC-003, task-layer task).
pub fn conclusions_positive(v: &View) -> Vec<(String, ConclusionType)> {
    closure::presentable(&v.closure.conclusions)
        .filter(|c| c.conclusion_type.is_positive() && !c.literal.negation)
        .map(|c| (lit_display(&c.literal), c.conclusion_type))
        .collect()
}
