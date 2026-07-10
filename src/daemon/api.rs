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
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct WatchEvent {
    pub theory: String,
    pub literal: String,
    pub old: String,
    pub new: String,
    pub at: String,
}

pub fn serve(paths: Paths, _ident: Identity, lock: super::DaemonLock) -> AppResult<()> {
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
            events,
            counters: Arc::new(Mutex::new(Counters::default())),
            write_gate: Arc::new(Mutex::new(())),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        };

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
            },
        })),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppendBody {
    entries: Vec<Entry>,
}

async fn append_entries(
    State(state): State<AppState>,
    AxPath(theory): AxPath<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
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
    let res = tokio::task::spawn_blocking(move || merge_and_close(&state2, &theory2, &req.entries))
        .await
        .unwrap_or_else(|e| Err(AppError::Internal(format!("join: {e}"))));
    match res {
        Ok(n) => (
            StatusCode::OK,
            Json(serde_json::json!({"v":1, "appended": n, "theory": theory})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.code(), "detail": e.to_string()})),
        ),
    }
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
    recompute_and_notify(state, &store)?;
    Ok(entries.len())
}

fn recompute_and_notify(state: &AppState, store: &TheoryStore) -> AppResult<()> {
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
