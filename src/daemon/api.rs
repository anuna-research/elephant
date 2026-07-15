//! Loopback control API (SPEC-002 CON-101). Single writer to the store;
//! merges pre-signed Entries, recomputes closure per batch, pushes watch
//! events (REQ-109).

use crate::core::envelope::Entry;
use crate::errors::{AppError, AppResult};
use crate::id::Identity;
use crate::paths::Paths;
use crate::store::{GENESIS_THEORY, TheoryStore};
use axum::Json;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    paths: Paths,
    token: String,
    started_at: std::time::Instant,
    /// last presented tags per theory, for watch diffing
    tags: Arc<Mutex<BTreeMap<String, BTreeMap<String, String>>>>,
    /// Reference views per theory for the near-miss advisory (SPEC-005
    /// NFR-402): a projection of the same cached closure the change watch
    /// uses, refreshed once per merged batch — never computed on the
    /// producer path. Cold (absent) → no advisory, never a delayed append.
    views: Arc<Mutex<BTreeMap<String, Arc<crate::core::vocab::VocabView>>>>,
    events: tokio::sync::broadcast::Sender<WatchEvent>,
    counters: Arc<Mutex<Counters>>,
    /// Serialises store writes: the daemon is the single writer (REQ-102),
    /// and concurrent handler tasks must not interleave open→append→flush.
    write_gate: Arc<Mutex<()>>,
    shutdown: Arc<tokio::sync::Notify>,
}

#[derive(Default)]
struct Counters {
    entries_merged: u64,
    quarantined: u64,
    closures_run: u64,
    /// SPEC-005 OBS-401 / SPEC-001 OBS-002 extension.
    advisories_emitted: u64,
    /// Reference-view reads on the advisory path (`elephant vocab` runs
    /// its own local closure and never touches the daemon).
    vocab_views_served: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct WatchEvent {
    pub theory: String,
    pub literal: String,
    pub old: String,
    pub new: String,
    pub at: String,
}

pub fn serve(paths: Paths, ident: Identity, lock: super::DaemonLock) -> AppResult<()> {
    let rt =
        tokio::runtime::Runtime::new().map_err(|e| AppError::Internal(format!("tokio: {e}")))?;
    rt.block_on(async move {
        let _lock = lock; // held for the duration of serve
        let token = super::generate_token();
        let (events, _) = tokio::sync::broadcast::channel(1024);
        let state = AppState {
            paths: paths.clone(),
            token: token.clone(),
            started_at: std::time::Instant::now(),
            tags: Arc::new(Mutex::new(BTreeMap::new())),
            views: Arc::new(Mutex::new(BTreeMap::new())),
            events,
            counters: Arc::new(Mutex::new(Counters::default())),
            write_gate: Arc::new(Mutex::new(())),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        };

        // Continuous P2P sync (SPEC-002 REQ-106/107/108), opt-in via
        // ELEPHANT_SYNC_INTERVAL. Runs as a background task tied to the same
        // shutdown signal; an endpoint-bind failure is logged, never fatal —
        // the loopback control plane serves regardless.
        //
        // A sync session that applies peer entries must refresh the theory's
        // reference view and tag cache exactly as an HTTP append does — the
        // sync path writes straight to the store and never touches AppState,
        // so without this the SPEC-005 advisory (NFR-402) and the change
        // watch would go stale after every sync. The session reports each
        // theory it changed on `refresh_tx`; a consumer recomputes it under
        // the write gate (via recompute_and_notify).
        let (refresh_tx, mut refresh_rx) =
            tokio::sync::mpsc::unbounded_channel::<String>();
        {
            let state = state.clone();
            tokio::spawn(async move {
                while let Some(theory_id) = refresh_rx.recv().await {
                    let state = state.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        match TheoryStore::open(&state.paths, &theory_id) {
                            Ok(store) => {
                                if let Err(e) = recompute_and_notify(&state, &store) {
                                    tracing::warn!(%theory_id, "post-sync refresh: {e}");
                                }
                            }
                            Err(e) => tracing::warn!(%theory_id, "post-sync open: {e}"),
                        }
                    })
                    .await;
                }
            });
        }
        let sync_task = crate::p2p::sync::configured_interval().map(|interval| {
            let paths = paths.clone();
            let ident = Arc::new(ident);
            // A dedicated shutdown, NOT the axum one: `notify_one` wakes a
            // single waiter, so sharing it would race the server's own
            // shutdown. We abort this task after the server stops.
            let shutdown = Arc::new(tokio::sync::Notify::new());
            tokio::spawn(async move {
                if let Err(e) =
                    crate::p2p::sync::run(paths, ident, shutdown, interval, Some(refresh_tx)).await
                {
                    tracing::warn!("continuous sync stopped: {e}");
                }
            })
        });

        let app = axum::Router::new()
            .route("/v1/ping", get(ping))
            .route("/v1/status", get(status))
            .route("/v1/theories/{id}/entries", post(append_entries))
            .route("/v1/theories/{id}/watch", get(watch))
            .route("/v1/stop", post(stop))
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| AppError::Transport(format!("bind: {e}")))?;
        let addr = listener
            .local_addr()
            .map_err(|e| AppError::Transport(e.to_string()))?;

        super::write_record(
            &paths,
            &super::DiscoveryRecord {
                pid: std::process::id(),
                addr: addr.to_string(),
                token,
                started_at: chrono::Utc::now().to_rfc3339(),
                version: crate::VERSION.to_string(),
                api_version: super::API_VERSION,
            },
        )?;
        tracing::info!(%addr, "elephantd listening");

        let shutdown = state.shutdown.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                tokio::select! {
                    _ = shutdown.notified() => {},
                    _ = tokio::signal::ctrl_c() => {},
                }
            })
            .await
            .map_err(|e| AppError::Transport(format!("serve: {e}")))?;
        // The control plane has stopped; stop the sync engine too.
        if let Some(task) = sync_task {
            task.abort();
        }
        let _ = std::fs::remove_file(paths.daemon_record());
        Ok(())
    })
}

fn authorized(state: &AppState, headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.as_bytes(), state.token.as_bytes()))
}

/// Length-independent, data-independent byte comparison: the token is a
/// secret, and a short-circuiting `==` would leak it by timing to a
/// co-located process. Mirrors the PAKE path's constant-time care.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn ping() -> impl IntoResponse {
    Json(serde_json::json!({"v": 1, "api_version": super::API_VERSION}))
}

async fn status(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"auth"})),
        );
    }
    let theories = crate::store::list_theories(&state.paths)
        .map(|v| {
            v.into_iter()
                .map(|(m, n)| serde_json::json!({"theory": m.theory_id, "alias": m.alias, "entries": n}))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let c = state.counters.lock().unwrap();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "v": 1,
            "pid": std::process::id(),
            "uptime_s": state.started_at.elapsed().as_secs(),
            "theories": theories,
            "counters": {
                "entries_merged": c.entries_merged,
                "quarantined": c.quarantined,
                "closures_run": c.closures_run,
                "advisories_emitted": c.advisories_emitted,
                "vocab_views_served": c.vocab_views_served,
            },
        })),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppendBody {
    entries: Vec<Entry>,
}

/// SPEC-002 CON-101 append advisory extension (0.1.3): the `advice` control
/// is a query parameter, enabled by default; a client's `--no-advice` sends
/// `?advice=false`. Parsed from the raw query string, NOT via a typed
/// extractor: the advisory control MUST NOT change the append's success or
/// status (SPEC-005 REQ-406), so a malformed or unexpected value must never
/// reject the request the way a `Query<T>` deserialization failure (400)
/// would. Only the exact `advice=false` disables the advisory; anything
/// else leaves it on, and the advisory computation degrades to no advisory
/// on its own if it cannot run.
fn advice_from_query(raw: Option<&str>) -> bool {
    let Some(q) = raw else { return true };
    for pair in q.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == "advice"
        {
            return v != "false";
        }
    }
    true
}

async fn append_entries(
    State(state): State<AppState>,
    AxPath(theory): AxPath<String>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let advice = advice_from_query(raw_query.as_deref());
    if !authorized(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"auth"})),
        );
    }
    if body.len() > 1_048_576 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error":"body over 1 MiB"})),
        );
    }
    let parsed: Result<AppendBody, _> = serde_json::from_slice(&body);
    let req = match parsed {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("bad body: {e}")})),
            );
        }
    };
    let state2 = state.clone();
    let theory2 = theory.clone();
    let res = tokio::task::spawn_blocking(move || {
        // SPEC-005 REQ-406: evaluate the advisory against the reference
        // view *before* applying the append (it must reflect exactly the
        // entries preceding this one); any failure degrades to None.
        let advisory = if advice {
            // Degradation clause (REQ-406): ANY failure of the advisory
            // computation — including a panic or a poisoned cache mutex —
            // yields no advisory, never a failed append.
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compute_advisory(&state2, &theory2, &req.entries)
            }))
            .ok()
            .flatten()
        } else {
            None
        };
        merge_and_close(&state2, &theory2, &req.entries).map(|n| (n, advisory))
    })
    .await
    .unwrap_or_else(|e| Err(AppError::Internal(format!("join: {e}"))));
    match res {
        Ok((n, advisory)) => {
            let mut obj = serde_json::json!({"v":1, "appended": n, "theory": theory});
            if let Some(a) = advisory {
                obj["advisory"] = a;
            }
            (StatusCode::OK, Json(obj))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.code(), "detail": e.to_string()})),
        ),
    }
}

/// Count top-level s-expressions in an SPL source, matching the lexer's
/// framing: `;` outside a string starts a line comment, a `"…"` atom may
/// contain escapes and balance-neutral parens, and each depth-0 list is one
/// form. A payload that `parse_spl` accepted has only list forms at top
/// level (a bare atom errors with "Expected list expression"), so counting
/// depth-0 open parens counts the forms exactly — the structural half of the
/// REQ-406 sole-fact precondition.
fn top_level_form_count(spl: &str) -> usize {
    let mut forms = 0usize;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut in_comment = false;
    for c in spl.chars() {
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            ';' => in_comment = true,
            '"' => in_string = true,
            '(' => {
                if depth == 0 {
                    forms += 1;
                }
                depth += 1;
            }
            ')' => depth = (depth - 1).max(0),
            _ => {}
        }
    }
    forms
}

/// REQ-406 preconditions + evaluation: a single pre-signed Entry whose act
/// is an assert whose payload's sole form is a fact, served from the cached
/// reference view. Everything here is best-effort — any miss returns None
/// and never blocks or delays the append (ADR-404).
fn compute_advisory(
    state: &AppState,
    theory: &str,
    entries: &[Entry],
) -> Option<serde_json::Value> {
    let [entry] = entries else { return None };
    let crate::core::envelope::SpeechAct::Assert { spl, .. } =
        crate::core::envelope::parse_wire(&entry.cbcl).ok()?
    else {
        return None;
    };
    // Sole form is a fact (REQ-406): the payload must be exactly ONE
    // top-level form, and that form a single-head fact rule. Counting
    // top-level forms is the primary guard — a no-op rider like a bodyless
    // `(claims x)`, `(#lang …)`, or `()` mutates none of the Theory
    // collections, so inferring "sole form" from collection-emptiness alone
    // lets a multi-form payload advise. The collection checks remain as a
    // belt-and-braces guard against a single form that carries more than a
    // bare fact.
    let t = spindle_parser::parse_spl(&spl).ok()?;
    if top_level_form_count(&spl) != 1 {
        return None;
    }
    let tp = t.trust_policy();
    let mut rules = t.rules();
    let fact = rules.next()?;
    if rules.next().is_some()
        || fact.rule_type != spindle_core::rule::RuleType::Fact
        || fact.head.len() != 1
        || !t.metadata().is_empty()
        || !t.predicate_metadata().is_empty()
        || !t.predicate_declarations().is_empty()
        || !t.superiorities().is_empty()
        || !tp.trust_map.is_empty()
        || !tp.thresholds.is_empty()
        || !tp.decay_map.is_empty()
    {
        return None;
    }
    let lit = fact.head.first()?.clone();

    // The reference view is keyed by theory id; the route may carry an
    // alias — resolve only if the direct lookup misses. The views mutex is
    // NEVER held across the alias resolution: `TheoryStore::open` reads and
    // imports the whole corpus snapshot, and `recompute_and_notify` (under
    // the write gate) must take this same lock to publish a refreshed view,
    // so holding it across the disk open would stall every theory's appends
    // (REQ-406: the advisory must not delay the append beyond the lookup).
    let view: Arc<crate::core::vocab::VocabView> = {
        let direct = state.views.lock().unwrap().get(theory).cloned();
        match direct {
            Some(v) => v,
            None => {
                let id = TheoryStore::open(&state.paths, theory).ok()?.theory_id;
                state.views.lock().unwrap().get(&id)?.clone()
            }
        }
    };
    {
        let mut c = state.counters.lock().unwrap();
        c.vocab_views_served += 1;
    }
    let advisory = crate::core::vocab::advisory(&view, &lit)?;
    {
        let mut c = state.counters.lock().unwrap();
        c.advisories_emitted += 1;
    }
    // OBS-401: the drift signal an operator tunes vocabulary against.
    tracing::info!(
        theory = %theory,
        kind = advisory.kind.name(),
        family = %advisory.family.rendered(),
        candidates = advisory.candidates.len(),
        "vocab advisory emitted"
    );
    Some(advisory.to_json())
}

/// Append entries, recompute closure, diff tags, broadcast watch events.
fn merge_and_close(state: &AppState, theory: &str, entries: &[Entry]) -> AppResult<usize> {
    let _gate = state.write_gate.lock().unwrap();
    let store = TheoryStore::open(&state.paths, theory)?;
    store.append_batch(entries)?;
    {
        let mut c = state.counters.lock().unwrap();
        c.entries_merged += entries.len() as u64;
    }
    // Gate already held: recompute inline so append + recompute stay atomic.
    recompute_locked(state, &store)?;
    Ok(entries.len())
}

/// Recompute a theory's closure, refresh its reference view and tag cache,
/// and broadcast watch events — taking the single-writer gate first. Called
/// off the append path (watch seed, p2p-sync refresh) where no gate is
/// already held; serialises against `merge_and_close` and against itself so
/// a slow recompute cannot publish a view older than a concurrent merge's.
fn recompute_and_notify(state: &AppState, store: &TheoryStore) -> AppResult<()> {
    let _gate = state.write_gate.lock().unwrap();
    recompute_locked(state, store)
}

/// The recompute body proper; the caller MUST already hold `write_gate`.
fn recompute_locked(state: &AppState, store: &TheoryStore) -> AppResult<()> {
    let (entries, _) = store.entries();
    let trust =
        std::fs::read_to_string(state.paths.trust_file(&store.theory_id)).unwrap_or_default();
    let resolve = store.key_resolver();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let closure = crate::core::closure::close(
        &entries,
        &store.theory_id,
        GENESIS_THEORY,
        &resolve,
        &trust,
        now_ms,
    )?;
    {
        let mut c = state.counters.lock().unwrap();
        c.closures_run += 1;
        c.quarantined += closure.quarantined.len() as u64;
    }
    // Refresh the SPEC-005 reference view on the same cycle as the tag
    // cache (NFR-402: staleness bounded by the change-watch batch).
    {
        let vocab_view = crate::core::vocab::view(&closure);
        state
            .views
            .lock()
            .unwrap()
            .insert(store.theory_id.clone(), Arc::new(vocab_view));
    }
    let new_tags: BTreeMap<String, String> = effective_tags(&closure);
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let mut all = state.tags.lock().unwrap();
    let old_tags = all.entry(store.theory_id.clone()).or_default();
    for (lit, new) in &new_tags {
        let old = old_tags.get(lit).cloned().unwrap_or_else(|| "-d".into());
        if &old != new {
            let _ = state.events.send(WatchEvent {
                theory: store.theory_id.clone(),
                literal: lit.clone(),
                old,
                new: new.clone(),
                at: at.clone(),
            });
        }
    }
    *old_tags = new_tags;
    Ok(())
}

/// Effective tag per literal (same collapse as `elephant status`).
pub fn effective_tags(closure: &crate::core::closure::Closure) -> BTreeMap<String, String> {
    use spindle_core::conclusion::ConclusionType;
    fn rank(t: ConclusionType) -> u8 {
        match t {
            ConclusionType::DefinitelyProvable => 0,
            ConclusionType::DefeasiblyProvable => 1,
            ConclusionType::DefinitelyNotProvable => 2,
            ConclusionType::DefeasiblyNotProvable => 3,
        }
    }
    let mut best: BTreeMap<String, ConclusionType> = BTreeMap::new();
    for c in crate::core::closure::presentable(&closure.conclusions) {
        let name = if c.literal.negation {
            format!("(not {})", c.literal.name())
        } else {
            c.literal.name().to_string()
        };
        best.entry(name)
            .and_modify(|t| {
                if rank(c.conclusion_type) < rank(*t) {
                    *t = c.conclusion_type;
                }
            })
            .or_insert(c.conclusion_type);
    }
    best.into_iter()
        .map(|(k, v)| (k, v.symbol().to_string()))
        .collect()
}

#[derive(Deserialize)]
struct WatchParams {
    literals: Option<String>,
}

async fn watch(
    State(state): State<AppState>,
    AxPath(theory): AxPath<String>,
    Query(params): Query<WatchParams>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    if !authorized(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, "auth").into_response();
    }
    // Resolve alias → id so filters match events.
    let theory_id = match TheoryStore::open(&state.paths, &theory) {
        Ok(s) => s.theory_id,
        Err(e) => return (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    };
    // Seed the tag cache so the first change diffs against reality.
    if let Ok(store) = TheoryStore::open(&state.paths, &theory_id) {
        let st = state.clone();
        let _ = tokio::task::spawn_blocking(move || recompute_and_notify(&st, &store)).await;
    }
    let filter: Option<Vec<String>> = params
        .literals
        .map(|l| l.split(',').map(|s| s.trim().to_string()).collect());
    let rx = state.events.subscribe();
    let stream = async_stream(rx, theory_id, filter);
    axum::response::Response::builder()
        .header("content-type", "application/x-ndjson")
        .body(axum::body::Body::from_stream(stream))
        .unwrap()
}

fn async_stream(
    mut rx: tokio::sync::broadcast::Receiver<WatchEvent>,
    theory: String,
    filter: Option<Vec<String>>,
) -> impl futures_core::Stream<Item = Result<String, std::io::Error>> + Send {
    async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if ev.theory != theory {
                        continue;
                    }
                    if let Some(f) = &filter {
                        if !f.iter().any(|l| l == &ev.literal) {
                            continue;
                        }
                    }
                    yield Ok(format!("{}\n", serde_json::to_string(&ev).unwrap()));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    }
}

async fn stop(State(state): State<AppState>, headers: axum::http::HeaderMap) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"auth"})),
        );
    }
    state.shutdown.notify_one();
    (
        StatusCode::OK,
        Json(serde_json::json!({"v":1, "stopping": true})),
    )
}

#[cfg(test)]
mod tests {
    use super::{advice_from_query, top_level_form_count};

    #[test]
    fn advice_query_is_infallible_and_only_false_disables() {
        // Only the exact advice=false disables; everything else keeps advice
        // on so a malformed value never rejects the append (REQ-406).
        assert!(advice_from_query(None));
        assert!(advice_from_query(Some("")));
        assert!(advice_from_query(Some("advice=true")));
        assert!(!advice_from_query(Some("advice=false")));
        // Malformed / unexpected values leave advice ON, never a 400.
        assert!(advice_from_query(Some("advice=1")));
        assert!(advice_from_query(Some("advice=yes")));
        assert!(advice_from_query(Some("advice=")));
        assert!(advice_from_query(Some("other=false")));
        // advice=false is honoured even after an unrelated leading pair.
        assert!(!advice_from_query(Some("x=1&advice=false")));
    }

    #[test]
    fn top_level_form_count_counts_depth_zero_forms() {
        assert_eq!(top_level_form_count("(given x)"), 1);
        // The REQ-406 multi-form bypasses: each is 2 top-level forms.
        assert_eq!(top_level_form_count("(given x) (claims mallory)"), 2);
        assert_eq!(top_level_form_count("(given x) (#lang spindle)"), 2);
        assert_eq!(top_level_form_count("(given x) ()"), 2);
        assert_eq!(top_level_form_count("(given x) (trusts a 1.0)"), 2);
        // Parens and comments inside a quoted atom do not add forms.
        assert_eq!(top_level_form_count(r#"(meta p (description "a (b) ; c"))"#), 1);
        // A comment between forms is not itself a form.
        assert_eq!(top_level_form_count("(given x) ; note\n"), 1);
    }
}
