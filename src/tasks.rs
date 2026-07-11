//! The hence-successor task layer (SPEC-003).
//!
//! Lifecycle SPL is byte-compatible with hence 0.7's chain-cancellation
//! bundles (ADR-201); the carrier is signed Entries instead of file appends.
//! Board states, next-action filtering and dependency propagation follow
//! hence's semantics exactly.

use crate::cli::Ctx;
use crate::errors::{AppError, AppResult};
use crate::queries::{View, view};
use std::collections::{BTreeMap, BTreeSet};

/// hence board precedence (SPEC-003 §1): first match wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Done,
    Blocked,
    UpstreamBlocked,
    Decomposed,
    InProgress,
    Ready,
    Backlog,
}

impl TaskState {
    fn bucket(&self) -> &'static str {
        match self {
            TaskState::Done => "done",
            TaskState::Blocked => "blocked",
            TaskState::UpstreamBlocked => "blocked",
            TaskState::Decomposed => "decomposed",
            TaskState::InProgress => "in_progress",
            TaskState::Ready => "ready",
            TaskState::Backlog => "backlog",
        }
    }
}

/// Everything the task layer needs from one closure pass.
pub struct TaskView {
    pub v: View,
    /// positive literal names (simple, non-negated).
    holds: BTreeSet<String>,
    /// declared task names, from `task-<name>` facts.
    pub tasks: Vec<String>,
}

pub fn task_view(ctx: &Ctx) -> AppResult<TaskView> {
    let v = view(ctx)?;
    let holds: BTreeSet<String> = crate::queries::conclusions_positive(&v)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    let mut tasks: Vec<String> = holds
        .iter()
        .filter_map(|l| l.strip_prefix("task-"))
        .map(str::to_string)
        .collect();
    tasks.sort();
    Ok(TaskView { v, holds, tasks })
}

impl TaskView {
    pub fn holds(&self, lit: &str) -> bool {
        self.holds.contains(lit)
    }

    /// hence state precedence.
    pub fn state(&self, task: &str) -> TaskState {
        if self.holds(&format!("completed-{task}")) {
            TaskState::Done
        } else if self.holds(&format!("blocked-{task}")) {
            TaskState::Blocked
        } else if self.holds(&format!("upstream-blocked-{task}")) {
            TaskState::UpstreamBlocked
        } else if self.holds(&format!("decomposed-{task}")) {
            TaskState::Decomposed
        } else if self.holds(&format!("claimed-{task}")) {
            TaskState::InProgress
        } else if self.holds(&format!("ready-{task}")) {
            TaskState::Ready
        } else {
            TaskState::Backlog
        }
    }

    /// `assign-to-<task>-<agent>` conclusions, disambiguated against the
    /// known task list (longest task match wins, as in hence).
    pub fn assignments(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for lit in &self.holds {
            let Some(rest) = lit.strip_prefix("assign-to-") else {
                continue;
            };
            let mut candidates: Vec<&String> = self
                .tasks
                .iter()
                .filter(|t| {
                    rest.strip_prefix(t.as_str())
                        .is_some_and(|r| r.starts_with('-'))
                })
                .collect();
            candidates.sort_by_key(|t| std::cmp::Reverse(t.len()));
            if let Some(task) = candidates.first() {
                let agent = rest[task.len() + 1..].to_string();
                out.push(((*task).clone(), agent));
            } else if let Some((task, agent)) = rest.rsplit_once('-') {
                out.push((task.to_string(), agent.to_string()));
            }
        }
        out.sort();
        out
    }

    /// Max version seen for an action prefix on a task (claim/unclaim/…),
    /// scanning the raw literal names (`<action>-vN-<task>`).
    fn max_version(&self, action: &str, task: &str) -> u32 {
        let mut max = 0;
        for lit in &self.holds {
            if let Some(rest) = lit.strip_prefix(&format!("{action}-v")) {
                if let Some((n, t)) = rest.split_once('-') {
                    if t == task {
                        if let Ok(n) = n.parse::<u32>() {
                            max = max.max(n);
                        }
                    }
                }
            }
        }
        max
    }

    /// Direct dependents: tasks whose readiness rule body mentions
    /// `completed-<task>` (hence's dependency extraction).
    pub fn dependents_of(&self, task: &str) -> Vec<String> {
        let needle = format!("completed-{task}");
        let mut out = BTreeSet::new();
        for rule in self.v.closure.theory.rules() {
            let Some(head) = rule.head.first() else {
                continue;
            };
            let Some(dep) = head.name().strip_prefix("ready-") else {
                continue;
            };
            let mentions = rule
                .body
                .iter()
                .filter_map(|b| b.as_logic())
                .any(|l| l.name() == needle && !l.negation);
            if mentions {
                out.insert(dep.to_string());
            }
        }
        out.into_iter().collect()
    }

    /// Transitive downstream closure over ready-rule dependencies.
    pub fn downstream_of(&self, task: &str) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut queue = self.dependents_of(task);
        while let Some(t) = queue.pop() {
            if seen.insert(t.clone()) {
                queue.extend(self.dependents_of(&t));
            }
        }
        seen.into_iter().collect()
    }

    fn rule_label_exists(&self, label: &str) -> bool {
        self.v.closure.theory.get_rule(label).is_some()
    }
}

// ── bundle generation (hence lifecycle.rs semantics, ADR-201) ──────────

/// hence's failure-propagation rules for one task's dependents.
fn propagation_rules(tv: &TaskView, task: &str, version: u32) -> Vec<String> {
    let mut out = Vec::new();
    for dep in tv.dependents_of(task) {
        for (suffix, premise) in [
            ("failed", format!("failed-{task}")),
            ("permanently-failed", format!("permanently-failed-{task}")),
            (
                &format!("stale-v{version}") as &str,
                format!("stale-v{version}-{task}"),
            ),
            (
                &format!("timeout-v{version}") as &str,
                format!("timeout-v{version}-{task}"),
            ),
            ("blocked", format!("upstream-blocked-{task}")),
        ] {
            let label = format!("r-propagate-{dep}-from-{task}-{suffix}");
            if !tv.rule_label_exists(&label) {
                out.push(format!(
                    "(normally {label} {premise} upstream-blocked-{dep})"
                ));
            }
        }
        // Transitive cascade from the dependent onward.
        for further in tv.downstream_of(&dep) {
            let label = format!("r-propagate-{further}-from-{dep}-blocked");
            if !tv.rule_label_exists(&label) {
                out.push(format!(
                    "(normally {label} upstream-blocked-{dep} upstream-blocked-{further})"
                ));
            }
        }
    }
    out
}

/// Claim bundle (REQ-201): action chain + optional cancel of prior unclaim.
fn claim_bundle(tv: &TaskView, task: &str) -> (u32, Vec<String>) {
    let n = tv
        .max_version("claim", task)
        .max(tv.max_version("unclaim", task))
        + 1;
    let m = tv.max_version("unclaim", task);
    let mut stmts = vec![
        format!("(given claim-v{n}-{task})"),
        format!("(normally r-cl-state-v{n}-{task} claim-v{n}-{task} state-claimed-v{n}-{task})"),
        format!("(normally r-cl-chain-v{n}-{task} state-claimed-v{n}-{task} claimed-{task})"),
    ];
    if m > 0 {
        stmts.push(format!(
            "(normally r-cl-cancel-v{m}-{task} claim-v{n}-{task} (not state-unclaimed-v{m}-{task}))"
        ));
        stmts.push(format!(
            "(prefer r-cl-cancel-v{m}-{task} r-ucl-state-v{m}-{task})"
        ));
        stmts.push(format!(
            "(prefer r-cl-chain-v{n}-{task} r-ucl-unclaimed-v{m}-{task})"
        ));
    }
    stmts.extend(propagation_rules(tv, task, n));
    (n, stmts)
}

/// Counter bundle (REQ-202/REQ-204): negate the state and the head with
/// superiority over the prior chain.
fn counter_bundle(
    tv: &TaskView,
    task: &str,
    action: &str,        // "unclaim" | "unblock"
    counter_of: &str,    // "claim" | "block"
    label: &str,         // "ucl" | "ubl"
    counter_label: &str, // "cl" | "bl"
    head: &str,          // "claimed" | "blocked"
) -> Vec<String> {
    let n = tv
        .max_version(action, task)
        .max(tv.max_version(counter_of, task))
        + 1;
    let p = tv.max_version(counter_of, task);
    let mut stmts = vec![
        format!("(given {action}-v{n}-{task})"),
        format!(
            "(normally r-{label}-state-v{n}-{task} {action}-v{n}-{task} state-un{head}-v{n}-{task})"
        ),
    ];
    if p > 0 {
        stmts.push(format!(
            "(normally r-{label}-cancel-v{p}-{task} {action}-v{n}-{task} (not state-{head}-v{p}-{task}))"
        ));
        stmts.push(format!(
            "(prefer r-{label}-cancel-v{p}-{task} r-{counter_label}-state-v{p}-{task})"
        ));
        stmts.push(format!(
            "(normally r-{label}-un{head}-v{n}-{task} {action}-v{n}-{task} (not {head}-{task}))"
        ));
        stmts.push(format!(
            "(prefer r-{label}-un{head}-v{n}-{task} r-{counter_label}-chain-v{p}-{task})"
        ));
    }
    stmts
}

/// Block bundle mirrors claim with bl- labels (REQ-204) plus the reason
/// fact hence 0.7 dropped.
fn block_bundle(tv: &TaskView, task: &str, reason: &str) -> Vec<String> {
    let n = tv
        .max_version("block", task)
        .max(tv.max_version("unblock", task))
        + 1;
    let m = tv.max_version("unblock", task);
    let escaped = reason.replace('\\', "\\\\").replace('"', "\\\"");
    let mut stmts = vec![
        format!("(given block-v{n}-{task})"),
        format!("(normally r-bl-state-v{n}-{task} block-v{n}-{task} state-blocked-v{n}-{task})"),
        format!("(normally r-bl-chain-v{n}-{task} state-blocked-v{n}-{task} blocked-{task})"),
        format!("(given (blocked-by {task} \"{escaped}\"))"),
    ];
    if m > 0 {
        stmts.push(format!(
            "(normally r-bl-cancel-v{m}-{task} block-v{n}-{task} (not state-unblocked-v{m}-{task}))"
        ));
        stmts.push(format!(
            "(prefer r-bl-cancel-v{m}-{task} r-ubl-state-v{m}-{task})"
        ));
        stmts.push(format!(
            "(prefer r-bl-chain-v{n}-{task} r-ubl-unblocked-v{m}-{task})"
        ));
    }
    stmts.extend(propagation_rules(tv, task, n));
    stmts
}

// ── command handlers ────────────────────────────────────────────────────

fn assert_bundle(ctx: &Ctx, stmts: &[String]) -> AppResult<usize> {
    for s in stmts {
        crate::core::envelope::validate_assert_payload(s)
            .map_err(|q| AppError::Internal(format!("generated SPL invalid ({q}): {s}")))?;
    }
    crate::cli::append_asserts(ctx, stmts)
}

fn require_task(tv: &TaskView, task: &str) -> AppResult<()> {
    if tv.tasks.iter().any(|t| t == task) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!(
            "no task '{task}' — declared tasks: {}",
            if tv.tasks.is_empty() {
                "(none)".to_string()
            } else {
                tv.tasks.join(", ")
            }
        )))
    }
}

pub fn claim(ctx: &Ctx, task: &str, force: bool) -> AppResult<()> {
    let tv = task_view(ctx)?;
    require_task(&tv, task)?;
    if tv.holds(&format!("claimed-{task}")) {
        return note(ctx, task, "claim", "already claimed — no-op");
    }
    if !force && !tv.holds(&format!("ready-{task}")) {
        return Err(AppError::Usage(format!(
            "task '{task}' is not ready (state: {:?}); use --force to override",
            tv.state(task)
        )));
    }
    let (version, stmts) = claim_bundle(&tv, task);
    let n = assert_bundle(ctx, &stmts)?;
    done(ctx, task, "claim", version, n)
}

pub fn unclaim(ctx: &Ctx, task: &str) -> AppResult<()> {
    let tv = task_view(ctx)?;
    require_task(&tv, task)?;
    if tv.holds(&format!("completed-{task}")) {
        return Err(AppError::Usage(format!(
            "task '{task}' is completed; unclaim refused"
        )));
    }
    if !tv.holds(&format!("claimed-{task}")) {
        return note(ctx, task, "unclaim", "not claimed — no-op");
    }
    let stmts = counter_bundle(&tv, task, "unclaim", "claim", "ucl", "cl", "claimed");
    let n = assert_bundle(ctx, &stmts)?;
    done(ctx, task, "unclaim", tv.max_version("claim", task), n)
}

pub fn complete(ctx: &Ctx, task: &str) -> AppResult<()> {
    let tv = task_view(ctx)?;
    require_task(&tv, task)?;
    if tv.holds(&format!("completed-{task}")) {
        return note(ctx, task, "complete", "already completed — no-op");
    }
    let n = assert_bundle(ctx, &[format!("(given completed-{task})")])?;
    // All-done detection (REQ-203) against the post-append view.
    let tv2 = task_view(ctx)?;
    let all_done = tv2.tasks.iter().all(|t| tv2.state(t) == TaskState::Done);
    if all_done && !ctx.json {
        println!("all {} tasks complete — the plan is done", tv2.tasks.len());
    }
    done(ctx, task, "complete", 0, n)
}

pub fn block(ctx: &Ctx, task: &str, reason: &str) -> AppResult<()> {
    let tv = task_view(ctx)?;
    require_task(&tv, task)?;
    if tv.holds(&format!("completed-{task}")) {
        return Err(AppError::Usage(format!(
            "task '{task}' is completed; block refused"
        )));
    }
    let stmts = block_bundle(&tv, task, reason);
    let n = assert_bundle(ctx, &stmts)?;
    done(ctx, task, "block", 0, n)
}

pub fn unblock(ctx: &Ctx, task: &str) -> AppResult<()> {
    let tv = task_view(ctx)?;
    require_task(&tv, task)?;
    if !tv.holds(&format!("blocked-{task}")) {
        return note(ctx, task, "unblock", "not blocked — no-op");
    }
    let stmts = counter_bundle(&tv, task, "unblock", "block", "ubl", "bl", "blocked");
    let n = assert_bundle(ctx, &stmts)?;
    done(ctx, task, "unblock", 0, n)
}

fn note(ctx: &Ctx, task: &str, action: &str, msg: &str) -> AppResult<()> {
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "task": task, "action": action, "noop": true, "note": msg})
        );
    } else {
        println!("{task}: {msg}");
    }
    Ok(())
}

fn done(ctx: &Ctx, task: &str, action: &str, version: u32, entries: usize) -> AppResult<()> {
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "task": task, "action": action,
                "version": version, "entries_appended": entries})
        );
    } else {
        println!("{action} {task} ({entries} entries)");
    }
    Ok(())
}

// ── board / next / info / join-as ──────────────────────────────────────

pub fn board(ctx: &Ctx, agent: Option<&str>) -> AppResult<()> {
    let tv = task_view(ctx)?;
    let assignments = tv.assignments();
    let mut buckets: BTreeMap<&str, Vec<serde_json::Value>> = BTreeMap::new();
    for b in [
        "backlog",
        "ready",
        "in_progress",
        "decomposed",
        "blocked",
        "done",
    ] {
        buckets.insert(b, Vec::new());
    }
    for task in &tv.tasks {
        let state = tv.state(task);
        let assignee = assignments
            .iter()
            .find(|(t, a)| t == task && agent.is_none_or(|g| g == a))
            .map(|(_, a)| a.clone());
        if let Some(filter) = agent {
            let assigned_to_agent = assignments.iter().any(|(t, a)| t == task && a == filter);
            if !assigned_to_agent {
                continue;
            }
        }
        let mut item = serde_json::json!({"task": task});
        if let Some(a) = assignee {
            item["assignee"] = serde_json::json!(a);
        }
        if state == TaskState::UpstreamBlocked {
            item["block_type"] = serde_json::json!("upstream_blocked");
        } else if state == TaskState::Blocked {
            item["block_type"] = serde_json::json!("blocked");
        }
        buckets.get_mut(state.bucket()).unwrap().push(item);
    }
    if ctx.json {
        let mut obj = serde_json::Map::new();
        for (k, v) in &buckets {
            obj.insert((*k).to_string(), serde_json::json!(v));
        }
        if let Some(a) = agent {
            obj.insert("agent".into(), serde_json::json!(a));
        }
        println!("{}", serde_json::Value::Object(obj));
    } else {
        for (bucket, items) in &buckets {
            if items.is_empty() {
                continue;
            }
            println!("{}:", bucket.to_uppercase().replace('_', "-"));
            for i in items {
                let assignee = i
                    .get("assignee")
                    .and_then(|a| a.as_str())
                    .map(|a| format!("  @{a}"))
                    .unwrap_or_default();
                println!("  {}{}", i["task"].as_str().unwrap(), assignee);
            }
        }
    }
    Ok(())
}

pub fn next(ctx: &Ctx, agent: Option<&str>) -> AppResult<()> {
    let tv = task_view(ctx)?;
    let assignments = tv.assignments();
    let actionable = |task: &str| tv.state(task) == TaskState::Ready;
    let mut actions: Vec<(String, String)> = assignments
        .iter()
        .filter(|(t, a)| actionable(t) && agent.is_none_or(|g| g == a))
        .cloned()
        .collect();
    let fallback = actions.is_empty();
    if fallback {
        // hence fallback: no ready task → all assignments unfiltered.
        actions = assignments.clone();
    }
    if ctx.json {
        let items: Vec<_> = actions
            .iter()
            .map(|(t, a)| {
                serde_json::json!({
                    "task": t, "agent": a, "literal": format!("assign-to-{t}-{a}"),
                    "command": format!("elephant claim {t} -t {}", tv.v.store.theory_id),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({"v":1, "agent": agent, "fallback": fallback,
                "next_actions": items})
        );
    } else if actions.is_empty() {
        println!("no assignments derivable — is anyone `join-as` available?");
    } else {
        for (i, (t, a)) in actions.iter().enumerate() {
            println!("{}. {t} @{a} — elephant claim {t}", i + 1);
        }
    }
    Ok(())
}

pub fn plan_info(ctx: &Ctx) -> AppResult<()> {
    let tv = task_view(ctx)?;
    let meta = tv.v.closure.theory.get_meta("plan");
    match meta {
        Some(m) => {
            let props: BTreeMap<String, String> = m
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), format!("{v:?}")))
                .collect();
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({"v":1, "theory": tv.v.store.theory_id, "plan": props})
                );
            } else {
                for (k, v) in props {
                    println!("{k:<12} {v}");
                }
            }
        }
        None => {
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({"v":1, "theory": tv.v.store.theory_id, "plan": null})
                );
            } else {
                println!("no (meta plan …) block in this theory");
            }
        }
    }
    Ok(())
}

pub fn join_as(ctx: &Ctx, agent_name: Option<&str>) -> AppResult<()> {
    let name = match agent_name {
        Some(n) => n.to_string(),
        None => crate::id::load(&ctx.paths)?.profile.name,
    };
    let n = crate::cli::append_asserts(ctx, &[format!("(given agent-{name}-available)")])?;
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "agent": name, "entries_appended": n})
        );
    } else {
        println!("agent-{name}-available asserted");
    }
    Ok(())
}

/// SPEC-003 REQ-209: the plan template seeded at theory create.
pub fn plan_template(name: &str, ts: &str) -> Vec<String> {
    vec![format!(
        "(meta plan (title \"{}\") (status \"active\") (created \"{}\"))",
        name.replace('"', "\\\""),
        ts
    )]
}
