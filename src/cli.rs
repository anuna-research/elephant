//! CLI surface (SPEC-001 REQ-001..024, SPEC-002 REQ-101.., SPEC-003 ADR-205).
//!
//! Every verb has exactly one flat spelling; noun groups survive only for
//! lifecycle administration (`id`, `theory`, `daemon`) where the noun
//! disambiguates the object (SPEC-003 ADR-205, superseding ADR-203).

use crate::errors::{AppError, AppResult, Exit};
use clap::{Args, Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "elephant",
    version,
    about = "Elephant-3000: speech-act coordination on shared defeasible theories",
    long_about = "elephant — Elephant 2000 made computable.\n\
        Agents exchange signed speech acts (assert, retract, promise, request, concede)\n\
        into shared append-only theories; conclusions, task states and commitment\n\
        fulfilment are derived by defeasible reasoning. An elephant never forgets.",
    after_help = "Examples:\n\
        \x20 elephant id create --name alice          create your signing identity\n\
        \x20 elephant theory create release-v1        mint a shared theory\n\
        \x20 elephant -t release-v1 assert qa-signed  sign a statement into it\n\
        \x20 elephant -t release-v1 promise released --by 2026-08-01T00:00:00Z\n\
        \x20 elephant -t release-v1 status            conclusions with proof tags\n\
        \x20 elephant -t release-v1 commitments       who promised what, and its state\n\n\
        Docs & support: https://git.anuna.io/anuna-research/elephant"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Emit machine-readable JSON on stdout (stable contract)
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
    /// Sync daemon lifecycle
    #[command(subcommand)]
    Daemon(DaemonCmd),
    /// Show the active store (ELEPHANT_HOME), identity, and held theories
    ///
    /// Names the resolved home, whether it came from `ELEPHANT_HOME` or the
    /// platform default, the local DID, and the theories held there — so
    /// "one home = one identity = one steward" is legible at a glance and a
    /// wrong or empty store is self-diagnosing (SPEC-001 CON-005).
    Info,

    // ── producers (SPEC-001 REQ-005..009) ──
    /// Assert an SPL statement (or bare literal) into a theory
    Assert(AssertArgs),
    /// Retract your own earlier statement by sentence-id
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
    /// Document a predicate symbol (validated vocabulary producer)
    ///
    /// Fully recognises the indicator and every property against the
    /// vocabulary documentation grammar (SPEC-005 CON-401) before anything
    /// is signed, then appends one signed assert Entry: a
    /// `(predicate …)` declaration when --arg is given, else a
    /// `(meta (predicate <functor> <arity>) …)` meta-target. Raw
    /// `elephant assert` stays legal and unvalidated; `define` is the
    /// recognising producer.
    ///
    /// Example: elephant -t release define ci-green/1 --arg task:symbol \
    ///          --desc "CI pipeline green for task ?t" --kind evidence
    Define(DefineArgs),

    // ── reads (SPEC-001 REQ-010..017, SPEC-003 REQ-208) ──
    /// All conclusions with proof tags
    Status {
        /// Show trust-weighted degrees and thresholds
        #[arg(long)]
        trust: bool,
        /// Also emit the canonical semantic closure fingerprint (#20)
        #[arg(long)]
        fingerprint: bool,
    },
    /// Semantic closure fingerprint & replica comparison (#20)
    #[command(subcommand)]
    Closure(ClosureCmd),
    /// Derivation of a provable literal
    Explain { literal: String },
    /// Why a literal is not provable
    #[command(name = "why-not")]
    WhyNot { literal: String },
    /// Minimal fact set that would prove the literal
    ///
    /// Abduction: searches (bounded) for the smallest sets of facts that,
    /// if asserted, would make the literal provable. Nothing is written —
    /// use it to answer "what is still missing before X holds?".
    ///
    /// Example: elephant -t release require release-ready
    Require { literal: String },
    /// Hypothetical evaluation without asserting
    ///
    /// Evaluates the goal as if the given facts were asserted, without
    /// writing anything to the theory, and reports which conclusions would
    /// change. The last argument is the goal; everything before it is a
    /// hypothetical fact.
    ///
    /// Example: elephant -t release what-if qa-signed legal-signed release-ready
    #[command(name = "what-if")]
    WhatIf {
        /// Facts… then the goal literal (last argument)
        #[arg(required = true, num_args = 2..)]
        facts_then_goal: Vec<String>,
    },
    /// Commitment ledger with derived states
    Commitments,
    /// Well-described work this theory says is available now
    ///
    /// Projects `(task ?x)` that is also `(ready ?x)`, is not `(completed ?x)`,
    /// and carries no outstanding commitment — with the description, acceptance
    /// criterion and source-bearing readiness rule an agent needs before
    /// promising (SPEC-006). Read-only: it never appends, promises, or syncs.
    ///
    /// Takes no positional argument. There is no plan file and no task
    /// lifecycle to recover (SPEC-003 ADR-206); the theory is selected with the
    /// global `-t`, exactly like every other read.
    Next,
    /// Inspect a single entry by its sentence-id
    ///
    /// The read counterpart to the sentence-ids that `assert`/`define` mint
    /// and `retract`/`concede --re` consume: prints the one entry — speech
    /// act, SPL/literal form, signer, HLC timestamp, in-reply-to, and status
    /// (active/retracted/shadowed) — instead of dumping the whole journal and
    /// filtering client-side. `--json` emits the raw entry object.
    ///
    /// Example: elephant -t release show s-322fd6e1474dda9e
    Show { sentence_id: String },
    /// The full journal — every entry, including quarantined
    ///
    /// Filters compose with AND semantics (#22); an empty match is a valid
    /// empty result, not an error.
    ///
    /// Example: elephant -t release log --status active --performative assert
    Log(LogFilter),
    /// Stream tag changes for a literal
    ///
    /// Prints one line whenever the literal's proof tag changes (e.g. -d →
    /// +D), watching a running daemon when there is one and polling local
    /// appends otherwise. Runs until interrupted with Ctrl-C.
    ///
    /// Example: elephant -t release watch release-ready
    Watch { literal: String },
    /// Definitions + provenance for rule labels
    Describe {
        #[arg(required = true)]
        labels: Vec<String>,
    },
    /// Full conclusion dump with firing order
    Trace,
    /// Layered graph of the entire theory
    ///
    /// Every literal is a node carrying its effective proof tag; every rule
    /// draws edges from its body literals to its head, so facts sit at the
    /// top and derived conclusions flow downward — a whole-theory dependency
    /// view. Cycles (legal in defeasible theories) and
    /// rule preferences are listed under the graph. `--json` emits the raw
    /// graph (nodes, edges with rule labels, superiorities, cycles).
    Dag {
        /// Show only the cone around this literal (its transitive
        /// dependencies and dependents)
        #[arg(long)]
        focus: Option<String>,
    },
    /// The theory's working vocabulary: families, roles, class, docs
    ///
    /// One row per predicate family occurring in the admitted theory or
    /// carrying documentation: its kind (predicate/legacy/malformed),
    /// roles (fact/head/body/goal), class (hole/orphan/active), built-in
    /// marker, and documentation with drift markers (malformed keys,
    /// detached docs, cross-signer redefinition). Derived from the corpus —
    /// never a pinned schema (SPEC-005 REQ-401).
    Vocab,
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
    Create { name: String },
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
    /// Active/retracted/retraction accounting for a theory (#22)
    ///
    /// `theory list` shows a single journal total that is easy to misread as a
    /// count of active statements. `inspect` breaks it down — active vs.
    /// retracted assertions, retraction entries, and setup (meta/member) vs.
    /// content assertions — from the same status semantics `log` uses.
    ///
    /// Example: elephant theory inspect rootclaim-grounded --json
    Inspect { theory: String },
    /// Remove a member (steward only): MLS-remove + rotate the corpus key
    Remove {
        theory: String,
        did: String,
        /// Skip the confirmation prompt (required in non-interactive use)
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ClosureCmd {
    /// Compute the canonical semantic closure fingerprint of a theory
    ///
    /// A versioned digest of every non-membership conclusion's proof tag. It is
    /// independent of journal merge order, JSON conclusion order, and identity:
    /// two replicas that reached the same closure fingerprint the same, and a
    /// single changed proof tag changes the digest. Membership, local aliases,
    /// theory ids, signers, and timestamps are excluded by the documented
    /// default (see `--json`).
    ///
    /// Example: elephant -t rootclaim-grounded closure fingerprint
    Fingerprint {
        /// Include the canonical `{tag} {literal}` sequence in the output
        #[arg(long)]
        sequence: bool,
    },
    /// Compare two `status --json` artefacts for semantic convergence
    ///
    /// Recomputes each file's closure fingerprint from its conclusions and
    /// reports whether they match. Exit 0 on match, 10 on a clean mismatch,
    /// and the usual non-zero codes on a read/parse failure.
    ///
    /// Example: elephant closure compare status-a.json status-b.json
    Compare {
        /// First `status --json` file
        a: std::path::PathBuf,
        /// Second `status --json` file
        b: std::path::PathBuf,
    },
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

/// Journal entry status, as a closed set (#22). A typo like `--status activ`
/// is rejected at parse time rather than succeeding with an empty, ambiguous
/// result.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq)]
#[value(rename_all = "lowercase")]
pub enum LogStatus {
    Active,
    Retracted,
    Shadowed,
    Quarantined,
}

impl LogStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LogStatus::Active => "active",
            LogStatus::Retracted => "retracted",
            LogStatus::Shadowed => "shadowed",
            LogStatus::Quarantined => "quarantined",
        }
    }
}

/// Speech-act performative, as a closed set (#22) — the values
/// `SpeechAct::performative()` emits.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq)]
#[value(rename_all = "lowercase")]
pub enum LogPerformative {
    Assert,
    Retract,
    Commit,
    Request,
    Concede,
    Query,
    Justify,
}

impl LogPerformative {
    pub fn as_str(self) -> &'static str {
        match self {
            LogPerformative::Assert => "assert",
            LogPerformative::Retract => "retract",
            LogPerformative::Commit => "commit",
            LogPerformative::Request => "request",
            LogPerformative::Concede => "concede",
            LogPerformative::Query => "query",
            LogPerformative::Justify => "justify",
        }
    }
}

/// `log` filters (#22). Each present filter must match (AND); absent filters
/// are unconstrained. A journal is deterministic after merge/restart, so the
/// filtered view is too.
#[derive(Args, Default)]
pub struct LogFilter {
    /// Entry status (closed set)
    #[arg(long, value_enum)]
    pub status: Option<LogStatus>,
    /// Speech act (closed set)
    #[arg(long, value_enum)]
    pub performative: Option<LogPerformative>,
    /// Signer DID (exact)
    #[arg(long)]
    pub signer: Option<String>,
    /// Sentence-id or stable entry-id (matches either)
    #[arg(long)]
    pub sid: Option<String>,
    /// Retraction target: keep only retracts whose target is this sentence-id
    #[arg(long)]
    pub retracts: Option<String>,
}

#[derive(Args)]
pub struct AssertArgs {
    /// SPL statement, or a bare literal (sugared to `(given …)`)
    pub spl: String,
    /// Suppress the daemon's near-miss vocabulary advisory (SPEC-005
    /// REQ-406); the append itself is unaffected either way
    #[arg(long = "no-advice")]
    pub no_advice: bool,
}

#[derive(Args)]
pub struct DefineArgs {
    /// Predicate indicator `functor/arity` (e.g. ci-green/1); arity ≥ 1
    pub indicator: String,
    /// Description, 1–512 bytes, no control characters
    #[arg(long = "desc", value_name = "TEXT")]
    pub desc: Option<String>,
    /// Vocabulary kind
    #[arg(long, value_name = "evidence|state|discovery")]
    pub kind: Option<String>,
    /// Expected asserter, 1–128 bytes (e.g. "role:ci")
    #[arg(long, value_name = "WHO")]
    pub asserter: Option<String>,
    /// Argument declaration NAME:SORT (repeatable; count must equal arity;
    /// sorts: symbol integer decimal float number any)
    #[arg(long = "arg", value_name = "NAME:SORT")]
    pub args: Vec<String>,
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
    // noq_udp WARNs on every unreachable candidate path — routine on
    // IPv4-only networks (iroh probes relays over IPv6 too); -v restores it.
    let default = match verbose {
        0 => "warn,noq_udp=error",
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
    /// Global `-v` count. In text mode, ≥1 opts reads into a more detailed
    /// rendering (e.g. status showing the canonical proof-state name — #26).
    pub verbose: u8,
}

fn dispatch(cli: Cli) -> AppResult<()> {
    // `closure compare` is fully offline — two self-contained `status --json`
    // files, no store — so route it before resolving a home. Otherwise a
    // machine with no `ELEPHANT_HOME`/`HOME` fails in `Paths::resolve()` (exit
    // 2) even when both absolute input files are perfectly readable (#20 review).
    if let Command::Closure(ClosureCmd::Compare { a, b }) = &cli.command {
        return crate::queries::closure_compare(cli.json, a, b);
    }
    let ctx = Ctx {
        paths: crate::paths::Paths::resolve()?,
        json: cli.json,
        theory: cli.theory,
        at: cli.at,
        verbose: cli.verbose,
    };
    // Arms are wired as IMPL-001 tasks land; anything unwired is NYI.
    match cli.command {
        Command::Id(cmd) => handle_id(&ctx, cmd),
        Command::Theory(cmd) => handle_theory(&ctx, cmd),
        Command::Assert(args) => produce_assert(&ctx, &args.spl, !args.no_advice),
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
        Command::Define(args) => produce_define(&ctx, &args),
        Command::Vocab => crate::queries::vocab(&ctx),
        Command::Status { trust, fingerprint } => crate::queries::status(&ctx, trust, fingerprint),
        Command::Closure(cmd) => match cmd {
            ClosureCmd::Fingerprint { sequence } => {
                crate::queries::closure_fingerprint(&ctx, sequence)
            }
            // Routed offline before store resolution (see top of dispatch);
            // this arm stays wired as the in-context fallback.
            ClosureCmd::Compare { a, b } => crate::queries::closure_compare(ctx.json, &a, &b),
        },
        Command::Explain { literal } => crate::queries::explain(&ctx, &literal),
        Command::WhyNot { literal } => crate::queries::why_not(&ctx, &literal),
        Command::Require { literal } => crate::queries::require(&ctx, &literal),
        Command::WhatIf { facts_then_goal } => crate::queries::what_if(&ctx, &facts_then_goal),
        Command::Commitments => crate::queries::commitments(&ctx),
        Command::Next => crate::queries::next(&ctx),
        Command::Show { sentence_id } => crate::queries::show(&ctx, &sentence_id),
        Command::Log(filter) => crate::queries::log(&ctx, &filter),
        Command::Describe { labels } => crate::queries::describe(&ctx, &labels),
        Command::Trace => crate::queries::trace(&ctx),
        Command::Dag { focus } => crate::dag::dag(&ctx, focus.as_deref()),
        Command::Daemon(cmd) => handle_daemon(&ctx, cmd),
        Command::Info => handle_info(&ctx),
        Command::Watch { literal } => watch_cmd(&ctx, &literal),
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
    /// Held until the entries are appended (locally or via the daemon): the
    /// HLC was allocated under this guard, and a concurrent same-identity
    /// writer must not sign the same one (its sentence id would collide).
    _clock: crate::store::ClockGuard,
}

/// Warn (to stderr) when a write targets an ephemeral home (#8): durable state
/// under `/tmp` is lost on reboot and nothing else signals it. Advisory only —
/// it never blocks the write and never touches the `--json` stdout contract.
fn warn_if_ephemeral(ctx: &Ctx) {
    if ctx.paths.is_ephemeral() {
        eprintln!(
            "warning: ELEPHANT_HOME is under an ephemeral path ({}) — \
             durable state here is lost on reboot",
            ctx.paths.home.display()
        );
    }
}

fn producer(ctx: &Ctx) -> AppResult<Producer> {
    refuse_at_on_write(ctx, "a producer command")?;
    warn_if_ephemeral(ctx);
    let theory = ctx
        .theory
        .as_deref()
        .ok_or_else(|| AppError::Usage("no theory given: pass -t <theory> (id or alias)".into()))?;
    let ident = crate::id::load(&ctx.paths)?;
    let mut store = TheoryStore::open(&ctx.paths, theory)?;
    // Apply pending MLS lane traffic first: a rotation delivered by sync
    // must land in the on-disk keybook before we seal anything.
    if crate::e2ee::process_mls_lane(&ctx.paths, &store)? {
        store = TheoryStore::open(&ctx.paths, theory)?;
    }
    store.bind_identity(&ident)?;
    let clock = store.lock_clock()?;
    let (wall_ms, ts) = now_pair();
    let hlc = store.tick(&ident, wall_ms);
    Ok(Producer {
        ident,
        store,
        hlc,
        ts,
        _clock: clock,
    })
}

fn append_act(ctx: &Ctx, p: &Producer, act: SpeechAct, spl_form: &str) -> AppResult<()> {
    append_act_with_advice(ctx, p, act, spl_form, false)
}

fn append_act_with_advice(
    ctx: &Ctx,
    p: &Producer,
    act: SpeechAct,
    spl_form: &str,
    advice: bool,
) -> AppResult<()> {
    let entry = Entry::create(
        &p.store.theory_id,
        p.hlc,
        p.ident.did.as_str(),
        &format!("{}#key-0", p.ident.did.as_str()),
        &act,
        &p.ts,
        &p.ident.signing_key,
    );
    let advisory = route_append(ctx, &p.store, std::slice::from_ref(&entry), advice)?;
    let receipt = match &act {
        SpeechAct::Assert { sentence_id, .. } | SpeechAct::Commit { sentence_id, .. } => {
            sentence_id.clone()
        }
        SpeechAct::Request { request_id, .. } => request_id.clone(),
        _ => Entry::sentence_id(&p.store.theory_id, &entry.signer, entry.hlc),
    };
    if ctx.json {
        let mut obj = serde_json::json!({
            "v": 1,
            "receipt": receipt,
            "theory": p.store.theory_id,
            "signer": entry.signer,
            "performative": act.performative(),
            "spl_form": spl_form,
        });
        if let Some(a) = &advisory {
            obj["advisory"] = a.clone();
        }
        println!("{obj}");
    } else {
        println!("{}  {}", act.performative(), receipt);
    }
    // REQ-406: text mode prints the advisory to stderr — advice, never a
    // verdict; the append above already succeeded.
    if !ctx.json
        && let Some(a) = &advisory
    {
        let esc = crate::core::vocab::escape_controls;
        let kind = a["kind"].as_str().unwrap_or("?");
        let family = a["family"].as_str().unwrap_or("?");
        eprintln!("advisory ({}): family {}", esc(kind), esc(family));
        if let Some(cands) = a["candidates"].as_array() {
            for c in cands {
                eprintln!(
                    "  did you mean {}?  (listener {})",
                    esc(c["literal"].as_str().unwrap_or("?")),
                    esc(c["listener"].as_str().unwrap_or("?")),
                );
            }
        }
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
    route_append(ctx, &p.store, &entries, false)?;
    Ok(entries.len())
}

/// Single-writer discipline (REQ-102): a live daemon owns store writes;
/// otherwise write directly. Returns the daemon's REQ-406 advisory when
/// one was emitted (direct-store mode never advises — NFR-402).
fn route_append(
    ctx: &Ctx,
    store: &TheoryStore,
    entries: &[Entry],
    advice: bool,
) -> AppResult<Option<serde_json::Value>> {
    if let Some(rec) = live_daemon(ctx) {
        let (_n, advisory) =
            crate::daemon::client::append(&rec, &store.theory_id, entries, advice)?;
        Ok(advisory)
    } else {
        store.append_batch(entries)?;
        Ok(None)
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

fn produce_assert(ctx: &Ctx, spl_in: &str, advice: bool) -> AppResult<()> {
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
    append_act_with_advice(
        ctx,
        &p,
        SpeechAct::Assert {
            sentence_id: sid,
            spl,
        },
        &spl_form,
        advice,
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

// ── define (SPEC-005 REQ-403, CON-401) ──────────────────────────────────

/// The recognising vocabulary producer: full recognition against CON-401
/// before anything is signed or stored (exit 3 on any refusal), then one
/// signed assert Entry through the ordinary producer path.
fn produce_define(ctx: &Ctx, args: &DefineArgs) -> AppResult<()> {
    use crate::core::vocab;
    let refuse = |m: String| AppError::Parse(m);

    // Indicator: spindle's recogniser owns the *form* (SPEC-024, one parser
    // per language) and already enforces canonical arity (no leading zero,
    // ≤ u32::MAX); take functor/arity from its `PredicateSymbol` rather than
    // re-splitting the string (a second grammar that could drift from
    // spindle's — e.g. quoted functors). CON-401 then constrains it further:
    // LDH functor, arity ≥ 1, not a built-in.
    let sym = spindle_parser::parse_predicate_indicator(&args.indicator).map_err(|e| {
        refuse(format!(
            "'{}' is not a predicate indicator (functor/arity): {e}",
            args.indicator
        ))
    })?;
    let functor = sym.functor().to_string();
    let arity = sym.arity() as u64;
    if !crate::store::is_valid_alias(&functor) {
        return Err(refuse(format!(
            "functor '{functor}' is not an LDH label (1-63 lowercase letters, \
             digits and hyphens — SPEC-001 REQ-003)"
        )));
    }
    if arity == 0 {
        return Err(refuse(format!(
            "'{functor}/0' is not a define target: a nullary predicate is \
             documented, if at all, as a legacy (meta {functor} …) assert on \
             the bare atom (SPEC-005 CON-402)"
        )));
    }
    if vocab::builtin(&functor).is_some() {
        return Err(refuse(format!(
            "'{functor}' is reserved built-in vocabulary at every arity \
             (SPEC-005 REQ-404); it cannot be redefined from the wire"
        )));
    }

    // Properties: ≥ 1 required; each checked against the CON-401 value
    // grammar, which admits any UTF-8 free of C0/C1 controls — including
    // quotes, backslashes and semicolons. Quoted values are escaped when
    // the payload is constructed (SPL quoted atoms unescape `\X` → `X`
    // and preserve `;` inside strings), and the round-trip check below
    // verifies the stored record is byte-exact.
    let props: Vec<(&str, &String, bool)> = [
        ("description", args.desc.as_ref(), true),
        ("kind", args.kind.as_ref(), false),
        ("asserter", args.asserter.as_ref(), true),
    ]
    .into_iter()
    .filter_map(|(k, v, q)| v.map(|v| (k, v, q)))
    .collect();
    if props.is_empty() {
        return Err(refuse(
            "define needs at least one property: --desc, --kind or --asserter".into(),
        ));
    }
    for (key, value, _) in &props {
        if !vocab::value_conforms(key, value) {
            return Err(refuse(format!(
                "--{} value does not conform to CON-401 ({})",
                if *key == "description" { "desc" } else { key },
                match *key {
                    "description" => "1-512 bytes UTF-8, no control characters",
                    "asserter" => "1-128 bytes UTF-8, no control characters",
                    _ => "one of: evidence, state, discovery",
                }
            )));
        }
    }

    // Argument declarations: count equals arity, unique LDH names,
    // primitive sorts.
    const SORTS: [&str; 6] = ["symbol", "integer", "decimal", "float", "number", "any"];
    let mut decls: Vec<(&str, &str)> = Vec::with_capacity(args.args.len());
    for spec in &args.args {
        let (name, sort) = spec
            .split_once(':')
            .ok_or_else(|| refuse(format!("--arg '{spec}' is not NAME:SORT")))?;
        if !crate::store::is_valid_alias(name) {
            return Err(refuse(format!("--arg name '{name}' is not an LDH label")));
        }
        if !SORTS.contains(&sort) {
            return Err(refuse(format!(
                "--arg sort '{sort}' is not primitive (symbol|integer|decimal|float|number|any)"
            )));
        }
        if decls.iter().any(|(n, _)| *n == name) {
            return Err(refuse(format!("--arg name '{name}' is not unique")));
        }
        decls.push((name, sort));
    }
    if !decls.is_empty() && decls.len() as u64 != arity {
        return Err(refuse(format!(
            "{} --arg declarations for arity {arity}: the counts must match",
            decls.len()
        )));
    }

    // Payload: a full declaration when --arg is supplied, else the bare
    // predicate meta-target (REQ-403).
    let prop_txt: String = props
        .iter()
        .map(|(k, v, quoted)| {
            if *quoted {
                // SPL quoted-atom escape: `\X` unescapes to `X`, so
                // backslash-doubling and quote-escaping round-trip any
                // CON-401-conforming value verbatim.
                let esc = v.replace('\\', "\\\\").replace('"', "\\\"");
                format!(" ({k} \"{esc}\")")
            } else {
                format!(" ({k} {v})")
            }
        })
        .collect();
    let payload = if decls.is_empty() {
        format!("(meta (predicate {functor} {arity}){prop_txt})")
    } else {
        let arg_txt: Vec<String> = decls.iter().map(|(n, s)| format!("({n} {s})")).collect();
        format!("(predicate {functor} ({}){prop_txt})", arg_txt.join(" "))
    };

    // Round-trip verification: parse the payload with the single SPL
    // recogniser and check the stored record is exactly what was asked
    // for — any value that smuggles structure fails here, before signing.
    let parsed = spindle_parser::parse_spl(&payload)
        .map_err(|e| refuse(format!("constructed payload does not parse: {e}")))?;
    // Reuse the indicator's own symbol — no need to rebuild it.
    let target = spindle_core::vocabulary::MetaTarget::Predicate(sym);
    let stored = parsed
        .get_meta_target(&target)
        .ok_or_else(|| refuse("payload round-trip lost the documentation record".into()))?;
    for (key, value, _) in &props {
        match stored.properties.get(*key) {
            Some(spindle_core::theory::MetaValue::String(s)) if s == *value => {}
            other => {
                return Err(refuse(format!(
                    "payload round-trip mismatch on '{key}': {other:?}"
                )));
            }
        }
    }
    if !decls.is_empty() {
        let ok = parsed
            .predicate_declarations()
            .iter()
            .any(|d| d.symbol() == sym);
        if !ok {
            return Err(refuse(
                "payload round-trip lost the predicate declaration".into(),
            ));
        }
    }

    // Rule/meta payloads and all other producers emit no advisory
    // (REQ-406) — the daemon would decline anyway; be explicit here.
    produce_assert(ctx, &payload, false)
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
        TheoryCmd::Create { name } => {
            refuse_at_on_write(ctx, "theory create")?;
            warn_if_ephemeral(ctx);
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
        TheoryCmd::Invite { theory, ttl } => {
            refuse_at_on_write(ctx, "theory invite")?;
            crate::p2p::run::invite(ctx, &theory, &ttl)
        }
        TheoryCmd::Join { code, alias } => {
            refuse_at_on_write(ctx, "theory join")?;
            warn_if_ephemeral(ctx);
            crate::p2p::run::join_theory(ctx, code.as_deref(), alias.as_deref())
        }
        TheoryCmd::Members { theory } => crate::p2p::run::members(ctx, &theory),
        TheoryCmd::Inspect { theory } => crate::queries::theory_inspect(ctx, &theory),
        TheoryCmd::Remove { theory, did, force } => {
            refuse_at_on_write(ctx, "theory remove")?;
            confirm_removal(&theory, &did, force)?;
            crate::p2p::run::remove_member(ctx, &theory, &did)
        }
    }
}

/// Removal is irreversible — it rotates the corpus key and locks the member
/// out of everything sealed afterwards — so confirm before acting. `--force`
/// is the non-interactive path; a prompt is never required (clig.dev).
fn confirm_removal(theory: &str, did: &str, force: bool) -> AppResult<()> {
    use std::io::{IsTerminal as _, Write as _};
    if force {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(AppError::Usage(
            "member removal needs confirmation: pass --force in non-interactive use".into(),
        ));
    }
    eprint!(
        "remove {did} from theory '{theory}'? \
         This rotates the corpus key and cannot be undone. [y/N] "
    );
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| AppError::Usage(format!("could not read confirmation: {e}")))?;
    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(AppError::Usage("removal cancelled".into())),
    }
}

/// `--at` is a read-time lens (REQ-021): it evaluates the closure at a
/// timestamp. Writes are always stamped with the current clock — silently
/// accepting `--at` would let a user believe they backdated an entry, and
/// the corpus is append-only, so that mistake would be permanent.
fn refuse_at_on_write(ctx: &Ctx, what: &str) -> AppResult<()> {
    if ctx.at.is_some() {
        return Err(AppError::Usage(format!(
            "--at only applies to read commands (status, log, explain, …); \
             {what} is always stamped with the current time"
        )));
    }
    Ok(())
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

/// `elephant info` — the active store, identity, and held theories (#8).
/// Makes the split-brain of "a machine with several homes" self-diagnosing:
/// which `ELEPHANT_HOME` am I on, whose identity, and what does it hold.
fn handle_info(ctx: &Ctx) -> AppResult<()> {
    let home = ctx.paths.home.display().to_string();
    let source = crate::paths::Paths::home_source();
    let ephemeral = ctx.paths.is_ephemeral();
    let ident = crate::id::load(&ctx.paths).ok();
    let theories = crate::store::list_theories(&ctx.paths).unwrap_or_default();

    if ctx.json {
        let items: Vec<_> = theories
            .iter()
            .map(
                |(m, n)| serde_json::json!({"theory": m.theory_id, "alias": m.alias, "entries": n}),
            )
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "v": 1,
                "store": home,
                "store_source": source,
                "ephemeral": ephemeral,
                "did": ident.as_ref().map(|i| i.did.to_string()),
                "name": ident.as_ref().map(|i| i.profile.name.clone()),
                "theories": items,
            })
        );
    } else {
        println!("store   {home}  ({source})");
        if ephemeral {
            println!("        ⚠ ephemeral home — state here is lost on reboot");
        }
        match &ident {
            Some(i) => println!("did     {}\nagent   {}", i.did, i.profile.name),
            None => println!("did     (no identity yet — `elephant id create`)"),
        }
        if theories.is_empty() {
            println!("theories (none)");
        } else {
            for (m, n) in &theories {
                println!("theory  {:<20} {:>5} entries  {}", m.alias, n, m.theory_id);
            }
        }
    }
    Ok(())
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
                        "running  pid {}  uptime {}s  theories {}  store {}",
                        st["pid"],
                        st["uptime_s"],
                        st["theories"].as_array().map(|a| a.len()).unwrap_or(0),
                        st["home"].as_str().unwrap_or("?"),
                    );
                    print_sync_health(&st);
                }
                Ok(())
            }
            other => {
                // Name the store searched (#8): "not running" for a shell on a
                // different home than its launchd daemon is otherwise
                // indistinguishable from a genuinely stopped daemon.
                let store = ctx.paths.home.display().to_string();
                if ctx.json {
                    println!(
                        "{}",
                        serde_json::json!({"v":1, "running": false,
                            "state": format!("{other:?}"), "store": store})
                    );
                } else {
                    println!("not running ({other:?}) in store {store}");
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

/// Render the daemon's per-peer sync health (#15) under `daemon status` text
/// output: one summary line per theory, then a line per peer with its
/// staleness, last successful sync, and last error.
fn print_sync_health(st: &serde_json::Value) {
    let Some(sync) = st["sync"].as_array() else {
        return;
    };
    for theory in sync {
        let peers = theory["peers"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let stale = peers.iter().filter(|p| p["stale"] == true).count();
        println!(
            "  sync {}: {} peer(s), {} stale",
            theory["theory"].as_str().unwrap_or("?"),
            peers.len(),
            stale,
        );
        for p in peers {
            let flag = if p["stale"] == true { "stale" } else { "ok" };
            let when = p["last_sync_ok"].as_str().unwrap_or("never");
            let err = p["last_error"]
                .as_str()
                .map(|e| format!("  last-error: {e}"))
                .unwrap_or_default();
            println!(
                "    {flag:<5} {}  last-ok {when}{err}",
                p["peer"].as_str().unwrap_or("?"),
            );
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
