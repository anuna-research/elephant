//! One preparation configuration for closure and hypothetical queries.

use crate::errors::{AppError, AppResult};
use spindle_core::{
    Literal, Theory,
    pipeline::{PipelineResult, PrepareOptions},
};

pub fn typed(literal: &Literal) -> spindle_contract::literal::LiteralStructJsonV2 {
    literal.into()
}

/// Injective identity, including argument types and temporal bounds.
pub fn literal_key(literal: &Literal) -> String {
    serde_json::to_string(&typed(literal)).expect("literal DTO is serializable")
}

/// Adapt an unweighted aggregate program without discarding audit metadata.
/// Source annotations are provenance, not a trust policy. Spindle currently
/// rejects even these annotations on aggregate input, so only the reasoning
/// copy omits them. The original theory remains available on Closure.
pub fn source(theory: &Theory, options: &mut PrepareOptions) -> AppResult<Theory> {
    if !spindle_core::aggregation::source::has_folds(theory) {
        return Ok(theory.clone());
    }
    let policy = options
        .trust_policy
        .as_ref()
        .unwrap_or(theory.trust_policy());
    if !policy.trust_map.is_empty()
        || !policy.thresholds.is_empty()
        || !policy.decay_map.is_empty()
        || policy.default_trust != 0.0
    {
        return Err(AppError::Reasoner(
            "Spindle does not support trust-weighted aggregation".into(),
        ));
    }
    // This is safe only for an atemporal program. Spindle's aggregate bridge
    // validates the rules and rejects temporal/modal inputs; we do not filter
    // them away or silently evaluate them without time.
    options.reference_time = None;
    options.trust_policy = None;
    let mut input = Theory::new();
    for rule in theory.rules() {
        input.add_rule(rule.clone());
    }
    for s in theory.superiorities() {
        input.add_superiority(&s.superior, &s.inferior);
    }
    for declaration in theory.predicate_declarations() {
        input.add_predicate_declaration(declaration.clone());
    }
    for (label, meta) in theory.metadata() {
        for (key, value) in &meta.properties {
            if key != "source" {
                input.add_meta(label, key, value.clone());
            }
        }
    }
    for (symbol, meta) in theory.predicate_metadata() {
        for (key, value) in &meta.properties {
            input.add_meta_target(
                spindle_core::vocabulary::MetaTarget::Predicate(*symbol),
                key,
                value.clone(),
            );
        }
    }
    Ok(input)
}

pub fn prepare(theory: &Theory, options: PrepareOptions) -> AppResult<PipelineResult> {
    let mut prepared = spindle_core::pipeline::prepare(theory, options)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    // The post-grounding temporal filter can drop priorities on template
    // labels because only grounded labels remain. Restore edges whose
    // endpoints still have live instances, retaining the time filter.
    let labels: std::collections::HashSet<String> = prepared
        .theory
        .rules()
        .flat_map(|r| [r.label.clone(), r.template_label().to_owned()])
        .collect();
    for s in theory.superiorities() {
        if labels.contains(&s.superior)
            && labels.contains(&s.inferior)
            && !prepared
                .theory
                .superiorities()
                .iter()
                .any(|p| p.superior == s.superior && p.inferior == s.inferior)
        {
            prepared.theory.add_superiority(&s.superior, &s.inferior);
        }
    }
    Ok(prepared)
}

/// Keep Elephant's flat `p arg` input as a compatibility spelling while using
/// Spindle's exact-one-ground-literal parser for all recognition.
pub fn parse_literal(text: &str) -> AppResult<Literal> {
    let text = text.trim();
    let direct = spindle_parser::parse_literal_input(text);
    direct
        .or_else(|original| {
            if !text.starts_with(['(', '"', '~'])
                && !text.contains(['(', ')', ';', '\n'])
                && text.split_whitespace().count() > 1
            {
                spindle_parser::parse_literal_input(&format!("({text})"))
            } else {
                Err(original)
            }
        })
        .map_err(|e| AppError::Parse(format!("'{text}' is not a valid ground literal: {e}")))
}

/// Raw candidates use the prepared rules (including bound extension results),
/// but verification always starts from source so new facts can change folds
/// and introduce new variable bindings.
pub fn abduce(
    closure: &super::closure::Closure,
    goal: &Literal,
    max: usize,
) -> AppResult<spindle_core::query::AbductionResult> {
    if max == 0 {
        return Err(AppError::Usage("max solutions must be at least 1".into()));
    }
    let mut result = spindle_core::query::abduce_with_conclusions(
        &closure.prepared.theory,
        goal,
        &closure.conclusions,
        max,
    )
    .map_err(|e| AppError::Reasoner(e.to_string()))?;
    if closure.has_aggregates && !result.is_already_provable() {
        // Lowering creates proof guards, not facts an operator can assert.
        // Never suggest forging one as a remedy for an aggregate result.
        result.solutions.retain(|s| {
            s.facts.iter().all(|f| {
                !f.name().starts_with("__aggregate_snapshot_")
                    || closure
                        .reasoning_theory
                        .rules()
                        .any(|r| r.head.iter().any(|h| h.name() == f.name()))
            })
        });
        if result.solutions.is_empty() {
            result
                .solutions
                .push(spindle_core::query::AbductionSolution::new(vec![
                    goal.clone(),
                ]));
        }
    }
    Ok(result)
}

pub fn what_if(
    closure: &super::closure::Closure,
    facts: Vec<spindle_core::query::HypotheticalClaim>,
    goal: &Literal,
) -> AppResult<spindle_core::query::WhatIfResult> {
    use spindle_core::{
        conclusion::{Conclusion, ConclusionType},
        query::{QueryResult, QueryStatus, WhatIfResult},
    };
    use std::collections::BTreeMap;
    // Inject before preparation: a hypothetical may create an entirely new
    // binding or change an aggregate's completed snapshot.
    let mut source = closure.reasoning_theory.clone();
    for (i, fact) in facts.iter().enumerate() {
        let mut label = format!("__elephant_hyp_{i}");
        while source.get_rule(&label).is_some() {
            label.push('_');
        }
        source.add_rule(spindle_core::Rule::fact(label, fact.literal.clone()));
    }
    let prepared = prepare(&source, closure.options.clone())?;
    let conclusions = spindle_core::reason::reason_prepared(&prepared.theory)
        .map_err(|e| AppError::Reasoner(e.to_string()))?;
    fn matches(expected: &Literal, candidate: &Literal) -> bool {
        spindle_core::projection::FamilyId::from(expected)
            == spindle_core::projection::FamilyId::from(candidate)
            && (!expected.is_temporal() || expected.temporal == candidate.temporal)
    }
    fn strength(tag: ConclusionType) -> u8 {
        match tag {
            ConclusionType::DefinitelyProvable => 4,
            ConclusionType::DefeasiblyProvable => 3,
            ConclusionType::DefeasiblyNotProvable => 2,
            ConclusionType::DefinitelyNotProvable => 1,
        }
    }
    fn strongest(conclusions: &[Conclusion]) -> BTreeMap<String, (Literal, ConclusionType)> {
        let mut result: BTreeMap<String, (Literal, ConclusionType)> = BTreeMap::new();
        for c in super::closure::presentable(conclusions) {
            result
                .entry(literal_key(&c.literal))
                .and_modify(|(_, tag)| {
                    if strength(c.conclusion_type) > strength(*tag) {
                        *tag = c.conclusion_type;
                    }
                })
                .or_insert_with(|| (c.literal.clone(), c.conclusion_type));
        }
        result
    }
    let mut result = QueryResult::new(goal.clone(), QueryStatus::Unknown);
    if let Some(c) = conclusions
        .iter()
        .filter(|c| c.is_positive() && matches(goal, &c.literal))
        .max_by_key(|c| strength(c.conclusion_type))
    {
        result = QueryResult::new(goal.clone(), QueryStatus::Provable)
            .with_conclusion_type(c.conclusion_type);
    } else if conclusions
        .iter()
        .any(|c| c.is_positive() && matches(&goal.complement(), &c.literal))
    {
        result = QueryResult::new(goal.clone(), QueryStatus::Refuted);
    }
    let before = strongest(&closure.conclusions);
    let after = strongest(&conclusions);
    let mut new_conclusions = Vec::new();
    let mut changed_conclusions = Vec::new();
    for (key, (literal, tag)) in after {
        if tag.is_positive() && !before.get(&key).is_some_and(|(_, old)| old.is_positive()) {
            new_conclusions.push(literal.clone());
        }
        if let Some((_, old)) = before.get(&key)
            && *old != tag
        {
            changed_conclusions.push((literal, *old, tag));
        }
    }
    Ok(WhatIfResult {
        hypotheticals: facts,
        result,
        new_conclusions,
        changed_conclusions,
    })
}

pub fn requires(
    closure: &super::closure::Closure,
    goal: &Literal,
    options: spindle_core::query::RequiresOptions,
) -> AppResult<spindle_core::query::RequiresResult> {
    use spindle_core::query::{
        HypotheticalClaim, RequiresResult, RequiresSearchStatus, RequiresVerificationStats,
    };
    if options.max_solutions == 0 || options.max_raw_candidates == 0 {
        return Err(AppError::Usage("query budgets must be at least 1".into()));
    }
    let raw = abduce(closure, goal, options.max_raw_candidates.saturating_add(1))?;
    let mut result = RequiresResult {
        already_provable: raw.is_already_provable(),
        solutions: Vec::new(),
        search_status: if raw.solutions.len() > options.max_raw_candidates {
            RequiresSearchStatus::BudgetExhausted
        } else {
            RequiresSearchStatus::BoundedComplete
        },
        verification: RequiresVerificationStats::default(),
    };
    if result.already_provable {
        return Ok(result);
    }
    let mut seen = std::collections::HashSet::new();
    for candidate in raw.solutions.into_iter().take(options.max_raw_candidates) {
        let mut keys: Vec<_> = candidate.facts.iter().map(literal_key).collect();
        keys.sort();
        if !seen.insert(keys) {
            continue;
        }
        result.verification.raw_examined += 1;
        let facts = candidate
            .facts
            .iter()
            .cloned()
            .map(HypotheticalClaim::new)
            .collect();
        if what_if(closure, facts, goal)?.is_provable() {
            result.verification.accepted += 1;
            result.solutions.push(candidate);
        } else {
            result.verification.rejected += 1;
        }
        if result.solutions.len() >= options.max_solutions {
            break;
        }
    }
    Ok(result)
}
