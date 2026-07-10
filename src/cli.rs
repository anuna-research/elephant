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
        _ => Err(AppError::Internal("not yet implemented".into())),
    }
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
            if template.is_some() {
                return Err(AppError::Internal(
                    "--template is not yet implemented (SPEC-003 REQ-209)".into(),
                ));
            }
            let ident = crate::id::load(&ctx.paths)?;
            let (wall_ms, ts) = now_pair();
            let store = crate::store::TheoryStore::create(&ctx.paths, &ident, &name, wall_ms, &ts)?;
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
