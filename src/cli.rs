//! CLI surface (SPEC-001 REQ-001..024, SPEC-002 REQ-101.., SPEC-003 ADR-203).
//!
//! Canonical groups mirror hence (`plan`, `task`, `query`); the flat
//! spellings (`elephant status`, `elephant why-not …`) are aliases of the
//! same implementations (SPEC-003 ADR-203).

use crate::errors::{AppError, AppResult, Exit};
use clap::{Args, Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "elephant",
    version,
    about = "Speech-act coordination on shared defeasible theories",
    long_about = "elephant — Elephant 2000 made computable.\n\
        Agents exchange signed speech acts (assert, retract, promise, request, concede)\n\
        into shared append-only theories; conclusions, task states and commitment\n\
        fulfilment are derived by defeasible reasoning. An elephant never forgets."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Emit machine-readable JSON on stdout (stable contract, CON-004)
    #[arg(long, global = true)]
    pub json: bool,

    /// Theory id or local alias
    #[arg(short = 't', long, global = true)]
    pub theory: Option<String>,

    /// Evaluate the theory as of this RFC 3339 timestamp
    #[arg(long, global = true, value_name = "RFC3339")]
    pub at: Option<String>,

    /// Verbose tracing to stderr (repeat for more)
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Subcommand)]
pub enum Command {
    /// Agent identity (DID, keys)
    #[command(subcommand)]
    Id(IdCmd),
    /// Theories: create, list, invite, join, members
    #[command(subcommand)]
    Theory(TheoryCmd),
    /// Plan views (hence-compatible): board, status, info
    #[command(subcommand)]
    Plan(PlanCmd),
    /// Task lifecycle (hence-compatible): next, claim, complete, …
    #[command(subcommand)]
    Task(TaskCmd),
    /// Queries (hence-compatible): explain, why-not, require, what-if, …
    #[command(subcommand)]
    Query(QueryCmd),
    /// Sync daemon lifecycle
    #[command(subcommand)]
    Daemon(DaemonCmd),

    // ── flat producers (SPEC-001 REQ-005..009) ──
    /// Assert an SPL statement (or bare literal) into a theory
    Assert(AssertArgs),
    /// Retract your own earlier statement by sentence-id (E1)
    Retract {
        sentence_id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Promise: commit to making a goal true (Elephant 2000 `commit`)
    Promise {
        /// Goal literal or SPL literal form
        goal: String,
        /// Trigger conditions (SPL body); defaults to true
        #[arg(long)]
        when: Option<String>,
        /// Deadline (RFC 3339)
        #[arg(long, value_name = "RFC3339")]
        by: Option<String>,
    },
    /// Request a commitment from another agent
    Request {
        /// Addressee (DID or member name)
        addressee: String,
        goal: String,
        #[arg(long)]
        when: Option<String>,
    },
    /// Concede a statement made by another agent
    Concede {
        literal: String,
        /// Sentence-id being conceded to
        #[arg(long = "re")]
        in_reply_to: String,
    },

    // ── flat consumers (aliases of plan/query group) ──
    /// All conclusions with proof tags (alias: plan status)
    Status {
        /// Show trust-weighted degrees and thresholds
        #[arg(long)]
        trust: bool,
    },
    /// Derivation of a provable literal (alias: query explain)
    Explain { literal: String },
    /// Why a literal is not provable (alias: query why-not)
    #[command(name = "why-not")]
    WhyNot { literal: String },
    /// Minimal fact set that would prove the literal (alias: query require)
    Require { literal: String },
    /// Hypothetical evaluation without asserting (alias: query what-if)
    #[command(name = "what-if")]
    WhatIf {
        /// Facts… then the goal literal (last argument)
        #[arg(required = true, num_args = 2..)]
        facts_then_goal: Vec<String>,
    },
    /// Commitment ledger with derived states (REQ-015)
    Commitments,
    /// The full journal — every entry, including quarantined (REQ-016)
    Log,
    /// Stream tag changes for a literal (REQ-017)
    Watch { literal: String },
}

#[derive(Subcommand)]
pub enum IdCmd {
    /// Create the local identity (Ed25519 key + did:crdt DID)
    Create {
        #[arg(long)]
        name: Option<String>,
    },
    /// Show DID, name, key path, public key
    Whoami,
}

#[derive(Subcommand)]
pub enum TheoryCmd {
    /// Create a new theory (mints genesis, derives theory id)
    Create {
        name: String,
        /// Seed with a template: plan (hence skeleton)
        #[arg(long)]
        template: Option<String>,
    },
    /// List locally-held theories
    List,
    /// Issue a single-use invite code (prints once; TTL bounded)
    Invite {
        theory: String,
        #[arg(long, default_value = "15m")]
        ttl: String,
    },
    /// Join a theory from an invite code (SPAKE2 ceremony)
    Join {
        /// Invite code (omit to be prompted on the TTY)
        code: Option<String>,
        #[arg(long)]
        alias: Option<String>,
    },
    /// List members (closure-derived roster)
    Members { theory: String },
}

#[derive(Subcommand)]
pub enum PlanCmd {
    /// Kanban board by task state (hence-compatible JSON)
    Board {
        #[arg(long)]
        agent: Option<String>,
    },
    /// All conclusions with proof tags
    Status {
        #[arg(long)]
        trust: bool,
    },
    /// Render the (meta plan …) block
    Info,
    /// Assert this agent as available for assignment rules
    #[command(name = "join-as")]
    JoinAs { agent_name: Option<String> },
}

#[derive(Subcommand)]
pub enum TaskCmd {
    /// Ready, unclaimed assignments for an agent
    Next {
        #[arg(long)]
        agent: Option<String>,
    },
    /// Claim a ready task (hence chain bundle)
    Claim {
        task: String,
        #[arg(long)]
        force: bool,
    },
    /// Withdraw a claim
    Unclaim { task: String },
    /// Assert completion
    Complete { task: String },
    /// Block a task with a reason
    Block { task: String, reason: String },
    /// Remove a block
    Unblock { task: String },
    /// Assert arbitrary SPL (same as flat `elephant assert`)
    Assert(AssertArgs),
}

#[derive(Subcommand)]
pub enum QueryCmd {
    Explain {
        literal: String,
    },
    #[command(name = "why-not")]
    WhyNot {
        literal: String,
    },
    Require {
        literal: String,
    },
    #[command(name = "what-if")]
    WhatIf {
        #[arg(required = true, num_args = 2..)]
        facts_then_goal: Vec<String>,
    },
    /// Definitions + provenance for rule labels
    Describe {
        #[arg(required = true)]
        labels: Vec<String>,
    },
    /// Full conclusion dump
    Trace,
}

#[derive(Subcommand)]
pub enum DaemonCmd {
    /// Start the daemon in the background
    Start,
    /// Run in the foreground
    Run,
    /// Liveness, theories, peers, counters
    Status,
    /// Stop a running daemon
    Stop,
}

#[derive(Args)]
pub struct AssertArgs {
    /// SPL statement, or a bare literal (sugared to `(given …)`)
    pub spl: String,
    /// Task annotation (hooks/metadata only)
    #[arg(long)]
    pub task: Option<String>,
}

/// Entry point: parse, init tracing, dispatch, render errors per REQ-024.
pub fn run() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // clap renders its own message; map to the usage exit code,
            // except help/version which are success.
            let _ = e.print();
            return match e.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    ExitCode::from(Exit::Success as u8)
                }
                _ => ExitCode::from(Exit::Usage as u8),
            };
        }
    };

    init_tracing(cli.verbose);

    let json = cli.json;
    match dispatch(cli) {
        Ok(()) => ExitCode::from(Exit::Success as u8),
        Err(err) => {
            if json {
                let obj = serde_json::json!({
                    "error": err.code(),
                    "exit": err.exit() as u8,
                    "detail": err.to_string(),
                });
                eprintln!("{obj}");
            } else {
                eprintln!("error: {err}");
            }
            ExitCode::from(err.exit() as u8)
        }
    }
}

fn init_tracing(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Per-invocation context handed to command handlers.
pub struct Ctx {
    pub paths: crate::paths::Paths,
    pub json: bool,
    pub theory: Option<String>,
    pub at: Option<String>,
}

fn dispatch(cli: Cli) -> AppResult<()> {
    let ctx = Ctx {
        paths: crate::paths::Paths::resolve()?,
        json: cli.json,
        theory: cli.theory,
        at: cli.at,
    };
    // Arms are wired as IMPL-001 tasks land; anything unwired is NYI.
    match cli.command {
        Command::Id(cmd) => handle_id(&ctx, cmd),
        Command::Theory(cmd) => handle_theory(&ctx, cmd),
        Command::Assert(args) => produce_assert(&ctx, &args.spl),
        Command::Task(TaskCmd::Assert(args)) => produce_assert(&ctx, &args.spl),
        Command::Retract {
            sentence_id,
            reason,
        } => produce_retract(&ctx, &sentence_id, reason.as_deref().unwrap_or("")),
        Command::Promise { goal, when, by } => {
            produce_promise(&ctx, &goal, when.as_deref(), by.as_deref())
        }
        Command::Request {
            addressee,
            goal,
            when,
        } => produce_request(&ctx, &addressee, &goal, when.as_deref()),
        Command::Concede {
            literal,
            in_reply_to,
        } => produce_concede(&ctx, &literal, &in_reply_to),
        Command::Status { trust } | Command::Plan(PlanCmd::Status { trust }) => {
            crate::queries::status(&ctx, trust)
        }
        Command::Explain { literal } | Command::Query(QueryCmd::Explain { literal }) => {
            crate::queries::explain(&ctx, &literal)
        }
        Command::WhyNot { literal } | Command::Query(QueryCmd::WhyNot { literal }) => {
            crate::queries::why_not(&ctx, &literal)
        }
        Command::Require { literal } | Command::Query(QueryCmd::Require { literal }) => {
            crate::queries::require(&ctx, &literal)
        }
        Command::WhatIf { facts_then_goal }
        | Command::Query(QueryCmd::WhatIf { facts_then_goal }) => {
            crate::queries::what_if(&ctx, &facts_then_goal)
        }
        Command::Commitments => crate::queries::commitments(&ctx),
        Command::Log => crate::queries::log(&ctx),
        Command::Query(QueryCmd::Describe { labels }) => crate::queries::describe(&ctx, &labels),
        Command::Query(QueryCmd::Trace) => crate::queries::trace(&ctx),
        Command::Plan(PlanCmd::Board { agent }) => crate::tasks::board(&ctx, agent.as_deref()),
        Command::Plan(PlanCmd::Info) => crate::tasks::plan_info(&ctx),
        Command::Plan(PlanCmd::JoinAs { agent_name }) => {
            crate::tasks::join_as(&ctx, agent_name.as_deref())
        }
        Command::Task(TaskCmd::Next { agent }) => crate::tasks::next(&ctx, agent.as_deref()),
        Command::Task(TaskCmd::Claim { task, force }) => crate::tasks::claim(&ctx, &task, force),
        Command::Task(TaskCmd::Unclaim { task }) => crate::tasks::unclaim(&ctx, &task),
        Command::Task(TaskCmd::Complete { task }) => crate::tasks::complete(&ctx, &task),
        Command::Task(TaskCmd::Block { task, reason }) => crate::tasks::block(&ctx, &task, &reason),
        Command::Task(TaskCmd::Unblock { task }) => crate::tasks::unblock(&ctx, &task),
        Command::Daemon(cmd) => handle_daemon(&ctx, cmd),
        Command::Watch { literal } => watch_cmd(&ctx, &literal),
        _ => Err(AppError::Internal("not yet implemented".into())),
    }
}

// ── producers (SPEC-001 REQ-005..009) ──────────────────────────────────

use crate::core::envelope::{Entry, SpeechAct};
use crate::store::TheoryStore;

struct Producer {
    ident: crate::id::Identity,
    store: TheoryStore,
    hlc: crate::core::envelope::Hlc,
    ts: String,
}

fn producer(ctx: &Ctx) -> AppResult<Producer> {
    let theory = ctx
        .theory
        .as_deref()
        .ok_or_else(|| AppError::Usage("no theory given: pass -t <theory> (id or alias)".into()))?;
    let ident = crate::id::load(&ctx.paths)?;
    let store = TheoryStore::open(&ctx.paths, theory)?;
    store.bind_identity(&ident)?;
    let (wall_ms, ts) = now_pair();
    let hlc = store.tick(&ident, wall_ms);
    Ok(Producer {
        ident,
        store,
        hlc,
        ts,
    })
}

fn append_act(ctx: &Ctx, p: &Producer, act: SpeechAct, spl_form: &str) -> AppResult<()> {
    let entry = Entry::create(
        &p.store.theory_id,
        p.hlc,
        p.ident.did.as_str(),
        &format!("{}#key-0", p.ident.did.as_str()),
        &act,
        &p.ts,
        &p.ident.signing_key,
    );
    route_append(ctx, &p.store, std::slice::from_ref(&entry))?;
    let receipt = match &act {
        SpeechAct::Assert { sentence_id, .. } | SpeechAct::Commit { sentence_id, .. } => {
            sentence_id.clone()
        }
        SpeechAct::Request { request_id, .. } => request_id.clone(),
        _ => Entry::sentence_id(&p.store.theory_id, &entry.signer, entry.hlc),
    };
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({
                "v": 1,
                "receipt": receipt,
                "theory": p.store.theory_id,
                "signer": entry.signer,
                "performative": act.performative(),
                "spl_form": spl_form,
            })
        );
    } else {
        println!("{}  {}", act.performative(), receipt);
    }
    Ok(())
}

/// Append a batch of assert speech-acts as one bundle (SPEC-003 ADR-202):
/// one Entry per statement, distinct HLCs, single Loro commit. Returns the
/// number of entries appended.
pub fn append_asserts(ctx: &Ctx, stmts: &[String]) -> AppResult<usize> {
    let p = producer(ctx)?;
    let mut entries = Vec::with_capacity(stmts.len());
    let mut hlc = p.hlc;
    for stmt in stmts {
        let sid = Entry::sentence_id(&p.store.theory_id, p.ident.did.as_str(), hlc);
        entries.push(Entry::create(
            &p.store.theory_id,
            hlc,
            p.ident.did.as_str(),
            &format!("{}#key-0", p.ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl: stmt.clone(),
            },
            &p.ts,
            &p.ident.signing_key,
        ));
        hlc.logical += 1;
    }
    route_append(ctx, &p.store, &entries)?;
    Ok(entries.len())
}

/// Single-writer discipline (REQ-102): a live daemon owns store writes;
/// otherwise write directly.
fn route_append(ctx: &Ctx, store: &TheoryStore, entries: &[Entry]) -> AppResult<()> {
    if let Some(rec) = live_daemon(ctx) {
        crate::daemon::client::append(&rec, &store.theory_id, entries)?;
        Ok(())
    } else {
        store.append_batch(entries)
    }
}

/// REQ-005: bare literals sugar to `(given …)`.
fn sugar_spl(input: &str) -> String {
    let t = input.trim();
    if t.starts_with('(') {
        t.to_string()
    } else {
        format!("(given {t})")
    }
}

fn produce_assert(ctx: &Ctx, spl_in: &str) -> AppResult<()> {
    let spl = sugar_spl(spl_in);
    // Full recognition before anything is signed or stored (CON-001).
    crate::core::envelope::validate_assert_payload(&spl)
        .map_err(|q| AppError::Parse(q.to_string()))?;
    let p = producer(ctx)?;
    let sid = Entry::sentence_id(&p.store.theory_id, p.ident.did.as_str(), p.hlc);
    let spl_form = spl
        .trim_start_matches('(')
        .split_whitespace()
        .next()
        .unwrap_or("?")
        .to_string();
    append_act(
        ctx,
        &p,
        SpeechAct::Assert {
            sentence_id: sid,
            spl,
        },
        &spl_form,
    )
}

fn produce_retract(ctx: &Ctx, target: &str, reason: &str) -> AppResult<()> {
    let p = producer(ctx)?;
    // Friendly pre-checks; the load-bearing E1 filter runs at closure time.
    let (entries, _) = p.store.entries();
    let target_entry = entries
        .iter()
        .find(|e| sentence_id_of(e).as_deref() == Some(target));
    match target_entry {
        None => {
            return Err(AppError::NotFound(format!(
                "no entry with sentence-id {target} in this theory"
            )));
        }
        Some(e) if e.signer != p.ident.did.as_str() => {
            return Err(AppError::E1(format!(
                "{target} was signed by {}; only its signer can retract it",
                e.signer
            )));
        }
        Some(_) => {}
    }
    append_act(
        ctx,
        &p,
        SpeechAct::Retract {
            target: target.to_string(),
            reason: reason.to_string(),
        },
        "retract",
    )
}

fn produce_promise(ctx: &Ctx, goal: &str, when: Option<&str>, by: Option<&str>) -> AppResult<()> {
    let trigger = when.unwrap_or("").trim().to_string();
    if !trigger.is_empty() {
        validate_spl_body(&trigger)?;
    }
    validate_spl_literal(goal)?;
    if let Some(ts) = by {
        let deadline = chrono::DateTime::parse_from_rfc3339(ts)
            .map_err(|e| AppError::Parse(format!("--by is not RFC 3339: {e}")))?;
        if deadline < chrono::Utc::now() {
            return Err(AppError::Parse(format!(
                "--by {ts} is in the past — a promise must have a future deadline"
            )));
        }
    }
    let p = producer(ctx)?;
    let sid = Entry::sentence_id(&p.store.theory_id, p.ident.did.as_str(), p.hlc);
    append_act(
        ctx,
        &p,
        SpeechAct::Commit {
            sentence_id: sid,
            trigger,
            by: by.map(str::to_string),
            goal: goal.trim().to_string(),
        },
        "commit",
    )
}

fn produce_request(ctx: &Ctx, addressee: &str, goal: &str, when: Option<&str>) -> AppResult<()> {
    let trigger = when.unwrap_or("").trim().to_string();
    if !trigger.is_empty() {
        validate_spl_body(&trigger)?;
    }
    validate_spl_literal(goal)?;
    let p = producer(ctx)?;
    let rid = Entry::sentence_id(&p.store.theory_id, p.ident.did.as_str(), p.hlc);
    append_act(
        ctx,
        &p,
        SpeechAct::Request {
            request_id: rid,
            addressee: addressee.to_string(),
            trigger,
            goal: goal.trim().to_string(),
        },
        "request",
    )
}

fn produce_concede(ctx: &Ctx, literal: &str, in_reply_to: &str) -> AppResult<()> {
    validate_spl_literal(literal)?;
    let p = producer(ctx)?;
    let (entries, _) = p.store.entries();
    let found = entries
        .iter()
        .any(|e| sentence_id_of(e).as_deref() == Some(in_reply_to));
    if !found {
        return Err(AppError::NotFound(format!(
            "--re {in_reply_to}: no such sentence in this theory"
        )));
    }
    append_act(
        ctx,
        &p,
        SpeechAct::Concede {
            literal: literal.trim().to_string(),
            in_reply_to: in_reply_to.to_string(),
        },
        "concede",
    )
}

/// The sentence-id an entry's own speech act carries (if any).
pub(crate) fn sentence_id_of(e: &Entry) -> Option<String> {
    match crate::core::envelope::parse_wire(&e.cbcl).ok()? {
        SpeechAct::Assert { sentence_id, .. } | SpeechAct::Commit { sentence_id, .. } => {
            Some(sentence_id)
        }
        SpeechAct::Request { request_id, .. } => Some(request_id),
        _ => None,
    }
}

/// Validate a body expression by wrapping it in a synthetic rule.
fn validate_spl_body(body: &str) -> AppResult<()> {
    spindle_parser::parse_spl(&format!("(normally __probe {body} __goal)"))
        .map(|_| ())
        .map_err(|e| AppError::Parse(format!("--when is not a valid SPL body: {e}")))
}

/// Validate a literal by wrapping it in a synthetic fact.
fn validate_spl_literal(lit: &str) -> AppResult<()> {
    let t = lit.trim();
    if t.is_empty() {
        return Err(AppError::Parse("empty literal".into()));
    }
    spindle_parser::parse_spl(&format!("(given {t})"))
        .map(|_| ())
        .map_err(|e| AppError::Parse(format!("'{t}' is not a valid SPL literal: {e}")))
}

pub fn now_pair() -> (u64, String) {
    let now = chrono::Utc::now();
    (
        now.timestamp_millis().max(0) as u64,
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
}

fn handle_theory(ctx: &Ctx, cmd: TheoryCmd) -> AppResult<()> {
    match cmd {
        TheoryCmd::Create { name, template } => {
            let seed = match template.as_deref() {
                None => Vec::new(),
                Some("plan") => crate::tasks::plan_template(&name, &now_pair().1),
                Some(other) => {
                    return Err(AppError::Usage(format!(
                        "unknown template '{other}' (available: plan)"
                    )));
                }
            };
            let ident = crate::id::load(&ctx.paths)?;
            let (wall_ms, ts) = now_pair();
            let store = crate::store::TheoryStore::create(&ctx.paths, &ident, &name, wall_ms, &ts)?;
            if !seed.is_empty() {
                let seed_ctx = Ctx {
                    paths: ctx.paths.clone(),
                    json: false,
                    theory: Some(store.theory_id.clone()),
                    at: None,
                };
                append_asserts(&seed_ctx, &seed)?;
            }
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "v": 1, "theory": store.theory_id, "alias": name, "created": ts,
                    })
                );
            } else {
                println!("created theory {name}");
                println!("id  {}", store.theory_id);
            }
            Ok(())
        }
        TheoryCmd::List => {
            let all = crate::store::list_theories(&ctx.paths)?;
            if ctx.json {
                let items: Vec<_> = all
                    .iter()
                    .map(|(m, n)| {
                        serde_json::json!({
                            "theory": m.theory_id, "alias": m.alias,
                            "created": m.created, "entries": n,
                        })
                    })
                    .collect();
                println!("{}", serde_json::json!({"v": 1, "theories": items}));
            } else if all.is_empty() {
                println!("no theories — `elephant theory create <name>` to start one");
            } else {
                for (m, n) in all {
                    println!("{:<24} {:>5} entries  {}", m.alias, n, m.theory_id);
                }
            }
            Ok(())
        }
        _ => Err(AppError::Internal(
            "not yet implemented (SPEC-002 join-p2p task)".into(),
        )),
    }
}

fn handle_id(ctx: &Ctx, cmd: IdCmd) -> AppResult<()> {
    match cmd {
        IdCmd::Create { name } => {
            let ident = crate::id::create(&ctx.paths, name)?;
            print_identity(ctx, &ident, true);
            Ok(())
        }
        IdCmd::Whoami => {
            let ident = crate::id::load(&ctx.paths)?;
            print_identity(ctx, &ident, false);
            Ok(())
        }
    }
}

fn print_identity(ctx: &Ctx, ident: &crate::id::Identity, created: bool) {
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({
                "v": 1,
                "did": ident.did.to_string(),
                "name": ident.profile.name,
                "key_path": ctx.paths.key_file(),
                "pubkey_multibase": ident.public_key_multibase(),
                "pubkey_hex": ident.public_key_hex(),
                "created": created,
            })
        );
    } else {
        if created {
            println!("created identity");
        }
        println!("{}", ident.did);
        println!("agent:{}", ident.profile.name);
        println!("key   {}", ctx.paths.key_file().display());
    }
}

// ── daemon lifecycle + watch (SPEC-002 REQ-101/102/109) ────────────────

fn handle_daemon(ctx: &Ctx, cmd: DaemonCmd) -> AppResult<()> {
    match cmd {
        DaemonCmd::Run => crate::daemon::run(&ctx.paths),
        DaemonCmd::Start => {
            let rec = crate::daemon::start(&ctx.paths)?;
            if ctx.json {
                println!(
                    "{}",
                    serde_json::json!({"v":1, "pid": rec.pid, "addr": rec.addr,
                        "started_at": rec.started_at})
                );
            } else {
                println!("daemon running (pid {}, {})", rec.pid, rec.addr);
            }
            Ok(())
        }
        DaemonCmd::Status => match crate::daemon::classify(&ctx.paths) {
            crate::daemon::Liveness::Live(rec) => {
                let st = crate::daemon::client::status(&rec)?;
                if ctx.json {
                    println!("{st}");
                } else {
                    println!(
                        "running  pid {}  uptime {}s  theories {}",
                        st["pid"],
                        st["uptime_s"],
                        st["theories"].as_array().map(|a| a.len()).unwrap_or(0)
                    );
                }
                Ok(())
            }
            other => {
                if ctx.json {
                    println!(
                        "{}",
                        serde_json::json!({"v":1, "running": false,
                            "state": format!("{other:?}")})
                    );
                } else {
                    println!("not running ({other:?})");
                }
                Ok(())
            }
        },
        DaemonCmd::Stop => {
            crate::daemon::stop(&ctx.paths)?;
            if ctx.json {
                println!("{}", serde_json::json!({"v":1, "stopped": true}));
            } else {
                println!("daemon stopped");
            }
            Ok(())
        }
    }
}

/// Live daemon record, if any (REQ-102 single-writer routing).
pub fn live_daemon(ctx: &Ctx) -> Option<crate::daemon::DiscoveryRecord> {
    match crate::daemon::classify(&ctx.paths) {
        crate::daemon::Liveness::Live(rec) => Some(rec),
        _ => None,
    }
}

/// REQ-017: stream tag changes until interrupted. With a live daemon this
/// is push-based; without, a local poll loop (direct mode).
fn watch_cmd(ctx: &Ctx, literal: &str) -> AppResult<()> {
    let store = crate::store::TheoryStore::open(
        &ctx.paths,
        ctx.theory
            .as_deref()
            .ok_or_else(|| AppError::Usage("no theory given: pass -t <theory>".into()))?,
    )?;
    let theory_id = store.theory_id.clone();
    drop(store);
    if let Some(rec) = live_daemon(ctx) {
        crate::daemon::client::watch(&rec, &theory_id, &[literal.to_string()], |ev| {
            if ctx.json {
                println!("{ev}");
            } else {
                println!(
                    "{}  {}: {} → {}",
                    ev["at"].as_str().unwrap_or(""),
                    ev["literal"].as_str().unwrap_or(""),
                    ev["old"].as_str().unwrap_or(""),
                    ev["new"].as_str().unwrap_or("")
                );
            }
        })
    } else {
        eprintln!("note: no daemon — watching local appends by polling");
        let mut last: Option<String> = None;
        loop {
            let v = crate::queries::view(ctx)?;
            let tags = crate::daemon::api::effective_tags(&v.closure);
            let tag = tags.get(literal).cloned().unwrap_or_else(|| "-d".into());
            if last.as_deref() != Some(tag.as_str()) {
                let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                if let Some(old) = &last {
                    if ctx.json {
                        println!(
                            "{}",
                            serde_json::json!({"theory": theory_id, "literal": literal,
                                "old": old, "new": tag, "at": at})
                        );
                    } else {
                        println!("{at}  {literal}: {old} → {tag}");
                    }
                }
                last = Some(tag);
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    }
}
