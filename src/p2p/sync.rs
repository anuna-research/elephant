//! Continuous P2P sync (SPEC-002 REQ-106/107/108): the steady-state loop that
//! keeps already-joined members converged. The daemon serves incoming sync
//! sessions (roster-gated) and periodically dials known roster peers.
//!
//! The transport-agnostic core — [`sync_as_initiator`], [`sync_as_responder`],
//! and the [`roster_node_pks`] gate — runs over any `AsyncRead + AsyncWrite`,
//! so it is exercised over an in-memory duplex in tests exactly as it runs
//! over an iroh QUIC bi-stream. The iroh glue ([`handle_connection`],
//! [`dial_once`], [`run`]) is the thin outer layer.

use super::{transport, wire};
use crate::errors::{AppError, AppResult};
use crate::id::Identity;
use crate::paths::Paths;
use crate::store::TheoryStore;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

/// The transport keys the theory's roster admits (REQ-107): the `node_pk` of
/// every closure-derived `member` fact. A sync peer is authorised iff its
/// authenticated transport key is in this set.
pub fn roster_node_pks(paths: &Paths, theory_id: &str) -> AppResult<HashSet<String>> {
    let store = TheoryStore::open(paths, theory_id)?;
    let (entries, _) = store.entries();
    let resolve = store.key_resolver();
    let trust = std::fs::read_to_string(paths.trust_file(theory_id)).unwrap_or_default();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let closure = crate::core::closure::close(
        &entries,
        theory_id,
        crate::store::GENESIS_THEORY,
        &resolve,
        &trust,
        now_ms,
    )?;
    let mut pks = HashSet::new();
    for c in crate::core::closure::presentable(&closure.conclusions) {
        if !c.conclusion_type.is_positive() || c.literal.negation {
            continue;
        }
        // A member fact reads `member "<did>" "<node_pk>"` (parens stripped by
        // to_spl's caller); take the second quoted field.
        let spl = c.literal.to_spl();
        let inner = spl
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(&spl);
        if let Some(rest) = inner.strip_prefix("member ") {
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if let [_did, node] = fields.as_slice() {
                pks.insert(node.trim_matches('"').to_string());
            }
        }
    }
    Ok(pks)
}

/// Drive a sync session as the INITIATOR (the dialer): open the theory, run
/// the session, persist merged deltas under the write lock, drain the MLS
/// lane. `theory_id` is known ahead of time.
pub async fn sync_as_initiator<S>(
    stream: &mut S,
    theory_id: &str,
    paths: &Paths,
) -> AppResult<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let store = TheoryStore::open(paths, theory_id)?;
    let applied = wire::sync_session(stream, theory_id, store.doc()).await?;
    store.commit_synced()?;
    let _ = crate::e2ee::process_mls_lane(paths, &store)?;
    Ok(applied)
}

/// Drive a sync session as the RESPONDER (the accept side): learn the theory
/// from the peer's opening offer, enforce the roster gate against the peer's
/// authenticated transport key (REQ-107), then complete the session. Returns
/// the theory that was synced and how many deltas were applied locally.
pub async fn sync_as_responder<S>(
    stream: &mut S,
    peer_node_pk: &str,
    paths: &Paths,
) -> AppResult<(String, usize)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (theory, peer_vv) = wire::read_opening_offer(stream).await?;
    // Roster gate: the peer must be a member of the theory it asks for. A
    // theory we do not hold locally has no roster and is refused.
    let roster = roster_node_pks(paths, &theory)?;
    if !roster.contains(peer_node_pk) {
        return Err(AppError::Signature(
            "sync refused: peer is not on this theory's roster".into(),
        ));
    }
    let store = TheoryStore::open(paths, &theory)?;
    let applied = wire::respond_sync_session(stream, &theory, store.doc(), &peer_vv).await?;
    store.commit_synced()?;
    let _ = crate::e2ee::process_mls_lane(paths, &store)?;
    Ok((theory, applied))
}

/// Notify the daemon that a sync session applied `applied` deltas to
/// `theory`, so it can refresh that theory's reference view and tag cache
/// (NFR-402). No-op when applied == 0 or no refresh channel is wired (e.g.
/// the standalone sync test harness).
fn note_synced(refresh: &Option<RefreshTx>, theory: &str, applied: usize) {
    if applied > 0
        && let Some(tx) = refresh
    {
        let _ = tx.send(theory.to_string());
    }
}

/// Channel the daemon listens on to recompute a theory after a sync session
/// applied peer entries (see `daemon::api::serve`).
pub type RefreshTx = tokio::sync::mpsc::UnboundedSender<String>;

// ── per-peer sync health (SPEC-002; #15) ────────────────────────────────

/// Health of the sync link to one peer for one theory. `daemon status`
/// exposes it so a node can locally tell it has diverged from a peer — the
/// session-scoped counters (`entries_merged`, resets per daemon run) cannot.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct PeerHealth {
    /// RFC 3339 timestamp of the last session that completed without error.
    pub last_sync_ok: Option<String>,
    /// Deltas applied on that last successful session.
    pub last_applied: u64,
    /// The last error message, and when it happened, if the most recent
    /// attempt (or a more recent one than the last success) failed.
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
}

/// Shared per-(theory, peer) health, updated by the sync loops and read by the
/// `daemon status` handler. `None` in the standalone sync test harness.
pub type SyncHealth = std::collections::BTreeMap<(String, String), PeerHealth>;
pub type HealthHandle = Arc<std::sync::Mutex<SyncHealth>>;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn record_ok(health: &Option<HealthHandle>, theory: &str, peer: &str, applied: usize) {
    if let Some(h) = health {
        let mut m = h.lock().unwrap();
        let e = m.entry((theory.to_string(), peer.to_string())).or_default();
        e.last_sync_ok = Some(now_rfc3339());
        e.last_applied = applied as u64;
        // A clean session clears the standing error.
        e.last_error = None;
        e.last_error_at = None;
    }
}

fn record_err(health: &Option<HealthHandle>, theory: &str, peer: &str, msg: &str) {
    if let Some(h) = health {
        let mut m = h.lock().unwrap();
        let e = m.entry((theory.to_string(), peer.to_string())).or_default();
        e.last_error = Some(msg.to_string());
        e.last_error_at = Some(now_rfc3339());
    }
}

// ── iroh glue ───────────────────────────────────────────────────────────

/// Serve one accepted sync connection: the roster gate keys off the QUIC
/// connection's authenticated remote id (REQ-107 — verified before any frame
/// is parsed for effect).
async fn handle_connection(
    conn: iroh::endpoint::Connection,
    paths: &Paths,
    refresh: &Option<RefreshTx>,
    health: &Option<HealthHandle>,
) -> AppResult<()> {
    let peer = conn.remote_id().to_string();
    let (send, recv) = conn
        .accept_bi()
        .await
        .map_err(|e| AppError::Transport(format!("accept sync stream: {e}")))?;
    let mut stream = tokio::io::join(recv, send);
    let (theory, applied) = sync_as_responder(&mut stream, &peer, paths).await?;
    // A completed accept is a successful sync with this peer, too.
    record_ok(health, &theory, &peer, applied);
    note_synced(refresh, &theory, applied);
    Ok(())
}

/// Max concurrent dials in one pass (#18): a large roster must not open an
/// unbounded number of sockets or spawn unbounded tasks.
const MAX_CONCURRENT_DIALS: usize = 8;

/// Per-peer dial budget (#18): one unreachable peer waits at most this long,
/// so it cannot add its full transport timeout to every other peer's cadence.
const PER_PEER_DIAL_TIMEOUT: Duration = Duration::from_secs(20);

/// The dial → open → session chain for one peer, collapsed to a single
/// per-peer outcome (deltas applied, or an error).
async fn dial_peer(
    endpoint: &iroh::Endpoint,
    paths: &Paths,
    theory: &str,
    peer_id: iroh::EndpointId,
) -> AppResult<usize> {
    let conn = transport::dial_sync(endpoint, peer_id)
        .await
        .map_err(|e| AppError::Transport(format!("dial peer: {e}")))?;
    let (send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| AppError::Transport(format!("open sync stream: {e}")))?;
    let mut stream = tokio::io::join(recv, send);
    sync_as_initiator(&mut stream, theory, paths).await
}

/// Concurrent, bounded dial scheduler (#18). Each `(theory, peer)` in `targets`
/// is dialled by `dial` on its own task with an independent
/// [`PER_PEER_DIAL_TIMEOUT`]; at most `concurrency` run at once. `on_result` is
/// invoked once per target, on this task, as each completes — so an offline
/// peer in one theory never delays the first attempt for a reachable peer in
/// another. The `JoinSet` aborts every in-flight dial if this future is dropped
/// (daemon shutdown cancels the whole sync task).
///
/// Generic over the dial operation so the scheduler is exercised deterministically
/// in tests (one never-completing dial next to an immediately-ready one) without
/// an iroh endpoint.
async fn dial_targets<K, F, Fut>(
    targets: Vec<(String, String, K)>,
    concurrency: usize,
    per_dial_timeout: Duration,
    dial: F,
    mut on_result: impl FnMut(String, String, Result<usize, String>),
) where
    K: Send + 'static,
    F: Fn(String, String, K) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<usize>> + Send + 'static,
{
    let limit = concurrency.max(1);
    let mut set: tokio::task::JoinSet<(String, String, Result<usize, String>)> =
        tokio::task::JoinSet::new();
    for (theory, pk, key) in targets {
        // Cap *live tasks*, not just sockets: reap a finished dial before
        // spawning past the limit, so a large roster cannot create an
        // unbounded number of tasks or sockets (#18). A completed task reaps
        // instantly; if all `limit` are still running, this waits for one.
        while set.len() >= limit {
            if let Some(Ok((theory, pk, outcome))) = set.join_next().await {
                on_result(theory, pk, outcome);
            }
        }
        let dial = dial.clone();
        set.spawn(async move {
            let outcome =
                tokio::time::timeout(per_dial_timeout, dial(theory.clone(), pk.clone(), key))
                    .await
                    .map_err(|_| "dial timed out".to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
            (theory, pk, outcome)
        });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok((theory, pk, outcome)) = joined {
            on_result(theory, pk, outcome);
        }
    }
}

/// One dial pass: snapshot every `(theory, roster-peer)` (except us) and dial
/// them concurrently with bounded fan-out. Best-effort — an offline or
/// unreachable peer times out and is recorded, never fatal, and never blocks
/// other peers (#18).
async fn dial_once(
    endpoint: &iroh::Endpoint,
    paths: &Paths,
    ident: &Identity,
    refresh: &Option<RefreshTx>,
    health: &Option<HealthHandle>,
) {
    let me = transport::node_pk(ident);
    let theories = match crate::store::list_theories(paths) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!("sync: cannot list theories: {e}");
            return;
        }
    };
    // Snapshot of due targets: (theory, peer_pk, resolved node id). A malformed
    // roster pk is skipped here (never a health error).
    let mut targets: Vec<(String, String, iroh::EndpointId)> = Vec::new();
    for (meta, _) in theories {
        let theory = meta.theory_id;
        for pk in roster_node_pks(paths, &theory).unwrap_or_default() {
            if pk == me {
                continue;
            }
            let Ok(peer_id) = transport::parse_node_pk(&pk) else {
                continue;
            };
            targets.push((theory.clone(), pk, peer_id));
        }
    }

    let endpoint = endpoint.clone();
    let paths = paths.clone();
    let dial = move |theory: String, _pk: String, peer_id: iroh::EndpointId| {
        let endpoint = endpoint.clone();
        let paths = paths.clone();
        async move { dial_peer(&endpoint, &paths, &theory, peer_id).await }
    };
    dial_targets(
        targets,
        MAX_CONCURRENT_DIALS,
        PER_PEER_DIAL_TIMEOUT,
        dial,
        |theory, pk, outcome| match outcome {
            Ok(applied) => {
                record_ok(health, &theory, &pk, applied);
                note_synced(refresh, &theory, applied);
            }
            Err(e) => {
                tracing::debug!(%theory, %pk, "sync failed: {e}");
                record_err(health, &theory, &pk, &e);
            }
        },
    )
    .await;
}

/// Run continuous sync until `shutdown` fires: bind the durable sync endpoint,
/// accept incoming sessions forever, and dial roster peers every `interval`.
pub async fn run(
    paths: Paths,
    ident: Arc<Identity>,
    shutdown: Arc<tokio::sync::Notify>,
    interval: Duration,
    refresh: Option<RefreshTx>,
    health: Option<HealthHandle>,
) -> AppResult<()> {
    let endpoint = transport::sync_endpoint(&ident).await?;
    run_with_endpoint(endpoint, paths, ident, shutdown, interval, refresh, health).await
}

/// The accept + dial loops over an already-bound endpoint. Split from [`run`]
/// so the live loopback test (tests/sync_live.rs) can drive the exact
/// production loops over an endpoint whose peer addresses are injected
/// instead of DHT-resolved.
pub async fn run_with_endpoint(
    endpoint: iroh::Endpoint,
    paths: Paths,
    ident: Arc<Identity>,
    shutdown: Arc<tokio::sync::Notify>,
    interval: Duration,
    refresh: Option<RefreshTx>,
    health: Option<HealthHandle>,
) -> AppResult<()> {
    tracing::info!(id = %endpoint.id(), "sync endpoint listening");

    // Accept loop.
    let accept = {
        let endpoint = endpoint.clone();
        let paths = paths.clone();
        let shutdown = shutdown.clone();
        let refresh = refresh.clone();
        let health = health.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.notified() => break,
                    incoming = endpoint.accept() => {
                        let Some(incoming) = incoming else { break };
                        let paths = paths.clone();
                        let refresh = refresh.clone();
                        let health = health.clone();
                        tokio::spawn(async move {
                            match incoming.await {
                                Ok(conn) => {
                                    if let Err(e) = handle_connection(conn, &paths, &refresh, &health).await {
                                        tracing::debug!("incoming sync failed: {e}");
                                    }
                                }
                                Err(e) => tracing::debug!("incoming handshake failed: {e}"),
                            }
                        });
                    }
                }
            }
        })
    };

    // Dial loop.
    loop {
        tokio::select! {
            _ = shutdown.notified() => break,
            _ = tokio::time::sleep(interval) => dial_once(&endpoint, &paths, &ident, &refresh, &health).await,
        }
    }
    accept.abort();
    endpoint.close().await;
    Ok(())
}

/// The dial interval from `ELEPHANT_SYNC_INTERVAL` (seconds). Absent, zero, or
/// unparseable → `None` (continuous sync disabled). Live P2P sync is opt-in
/// because it cannot be exercised in CI; see the design doc.
pub fn configured_interval() -> Option<Duration> {
    let raw = std::env::var("ELEPHANT_SYNC_INTERVAL").ok()?;
    let secs: u64 = raw.trim().parse().ok()?;
    (secs > 0).then(|| Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::envelope::{Entry, SpeechAct};
    use crate::e2ee::{self, MlsIdentity};

    /// Records `(peer, dial outcome)` pairs captured by a dial-scheduler sink.
    type DialOutcomes = std::sync::Arc<std::sync::Mutex<Vec<(String, Result<usize, String>)>>>;

    struct Node {
        _dir: tempfile::TempDir,
        paths: Paths,
        ident: Identity,
    }

    fn node(name: &str) -> Node {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, Some(name.into())).unwrap();
        Node {
            _dir: dir,
            paths,
            ident,
        }
    }

    /// Admit `m` to `steward`'s theory (the join ceremony's effect, minus the
    /// network), recording a roster member fact with m's transport key so the
    /// gate can find it. Returns the theory id.
    fn admit(steward: &Node, s_store: &TheoryStore, m: &Node) -> String {
        let theory_id = s_store.theory_id.clone();
        let s_provider = e2ee::open_provider(&steward.paths, &theory_id).unwrap();
        let s_mls = MlsIdentity::for_theory(&steward.ident, &theory_id);
        let mut s_group = e2ee::load_group(&s_provider, &theory_id).unwrap().unwrap();
        let m_provider = e2ee::open_provider(&m.paths, &theory_id).unwrap();
        let m_mls = MlsIdentity::for_theory(&m.ident, &theory_id);

        let (_b, kp) = e2ee::build_key_package(&m_provider, &m_mls).unwrap();
        let kp = e2ee::key_package_from_bytes(&s_provider, &kp, m.ident.did.as_str()).unwrap();
        let (commit, welcome) = e2ee::add_member(&s_provider, &mut s_group, &s_mls, kp).unwrap();
        s_store.push_mls(&[commit]).unwrap();
        e2ee::clear_outbox(&steward.paths, &theory_id).unwrap();
        e2ee::join_from_welcome(&m_provider, &welcome, steward.ident.did.as_str()).unwrap();

        // Roster fact naming m's transport key (what the gate checks).
        let node_pk = transport::node_pk(&m.ident);
        s_store.bind_identity(&steward.ident).unwrap();
        let clock = s_store.lock_clock().unwrap();
        let (wall_ms, ts) = crate::cli::now_pair();
        let hlc = s_store.tick(&steward.ident, wall_ms);
        let sid = Entry::sentence_id(&theory_id, steward.ident.did.as_str(), hlc);
        let entry = Entry::create(
            &theory_id,
            hlc,
            steward.ident.did.as_str(),
            &format!("{}#key-0", steward.ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl: format!(
                    "(given (member \"{}\" \"{node_pk}\"))",
                    m.ident.did.as_str()
                ),
            },
            &ts,
            &steward.ident.signing_key,
        );
        s_store.append(&entry).unwrap();
        drop(clock);
        theory_id
    }

    /// #15: record_ok/record_err maintain per-(theory, peer) health — a
    /// success stamps last_sync_ok and clears any standing error; an error
    /// stamps last_error while retaining the prior last_sync_ok.
    #[test]
    fn sync_health_records_ok_and_error() {
        let h: HealthHandle = Arc::new(std::sync::Mutex::new(SyncHealth::default()));
        let hopt = Some(h.clone());
        let key = ("t1".to_string(), "peerA".to_string());

        record_ok(&hopt, "t1", "peerA", 5);
        {
            let m = h.lock().unwrap();
            let e = &m[&key];
            assert_eq!(e.last_applied, 5);
            assert!(e.last_sync_ok.is_some());
            assert!(e.last_error.is_none());
        }

        record_err(&hopt, "t1", "peerA", "connection lost");
        {
            let m = h.lock().unwrap();
            let e = &m[&key];
            assert_eq!(e.last_error.as_deref(), Some("connection lost"));
            assert!(e.last_error_at.is_some());
            assert!(e.last_sync_ok.is_some(), "prior success is retained");
        }

        record_ok(&hopt, "t1", "peerA", 0);
        {
            let m = h.lock().unwrap();
            assert!(
                m[&key].last_error.is_none(),
                "a clean session clears the error"
            );
        }
    }

    /// #18: the concurrent dial scheduler does not let one hung peer delay a
    /// reachable one. A never-completing dial and an immediately-ready dial run
    /// together; the ready peer is recorded first (it completes at once, the
    /// hung peer only at the per-dial timeout), and the hung peer is recorded
    /// as a timeout rather than lost.
    #[tokio::test]
    async fn dial_scheduler_ready_peer_not_blocked_by_hung_peer() {
        use std::sync::Mutex;
        let targets = vec![
            ("tA".to_string(), "hung".to_string(), ()),
            ("tB".to_string(), "ready".to_string(), ()),
        ];
        let recorded: DialOutcomes = Arc::new(Mutex::new(Vec::new()));
        let sink = recorded.clone();
        dial_targets(
            targets,
            8,
            Duration::from_millis(150),
            |_theory, pk, _key: ()| async move {
                if pk == "hung" {
                    std::future::pending::<AppResult<usize>>().await
                } else {
                    Ok(7)
                }
            },
            move |_theory, pk, outcome| sink.lock().unwrap().push((pk, outcome)),
        )
        .await;

        let recorded = recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        // Ready peer completes immediately and is recorded first.
        assert_eq!(recorded[0].0, "ready");
        assert_eq!(recorded[0].1, Ok(7));
        // Hung peer is recorded as a timeout, not dropped.
        assert_eq!(recorded[1].0, "hung");
        assert_eq!(recorded[1].1, Err("dial timed out".to_string()));
    }

    /// REQ-108 over a duplex: an initiator and a roster-gated responder run a
    /// full session and both replicas converge to equal version vectors.
    #[tokio::test]
    async fn initiator_and_responder_converge() {
        let alice = node("alice");
        let a_store = TheoryStore::create(
            &alice.paths,
            &alice.ident,
            "t",
            1_784_000_000_000,
            "2026-07-11T00:00:00Z",
        )
        .unwrap();
        let bob = node("bob");
        let theory_id = admit(&alice, &a_store, &bob);

        // Bob materialises the theory (as the join ceremony would).
        let a_snapshot = a_store.doc().export(loro::ExportMode::Snapshot).unwrap();
        let keybook = crate::e2ee::keybook::Keybook::load(&crate::e2ee::keybook::keybook_path(
            &alice.paths,
            &theory_id,
        ))
        .unwrap()
        .unwrap();
        let b_store = TheoryStore::adopt(
            &bob.paths,
            &bob.ident,
            &theory_id,
            "t",
            keybook,
            alice.ident.did.as_str(),
            &alice.ident.document.to_bytes().unwrap(),
        )
        .unwrap();
        b_store.doc().import(&a_snapshot).unwrap();
        b_store.flush_public().unwrap();

        // Alice writes a NEW fact after bob's initial copy — only continuous
        // sync can carry it over.
        a_store.bind_identity(&alice.ident).unwrap();
        let clock = a_store.lock_clock().unwrap();
        let (wall_ms, ts) = crate::cli::now_pair();
        let hlc = a_store.tick(&alice.ident, wall_ms);
        let sid = Entry::sentence_id(&theory_id, alice.ident.did.as_str(), hlc);
        a_store
            .append(&Entry::create(
                &theory_id,
                hlc,
                alice.ident.did.as_str(),
                &format!("{}#key-0", alice.ident.did.as_str()),
                &SpeechAct::Assert {
                    sentence_id: sid,
                    spl: "(given post-copy-fact)".into(),
                },
                &ts,
                &alice.ident.signing_key,
            ))
            .unwrap();
        drop(clock);

        // Bob dials Alice; Alice responds (gated on bob's key).
        let (mut sa, mut sb) = tokio::io::duplex(1 << 20);
        let bob_pk = transport::node_pk(&bob.ident);
        let a_paths = alice.paths.clone();
        let responder =
            tokio::spawn(async move { sync_as_responder(&mut sa, &bob_pk, &a_paths).await });
        let b_paths = bob.paths.clone();
        let b_theory = theory_id.clone();
        let initiator =
            tokio::spawn(async move { sync_as_initiator(&mut sb, &b_theory, &b_paths).await });
        let (_theory, _applied) = responder.await.unwrap().unwrap();
        initiator.await.unwrap().unwrap();

        // Bob now sees Alice's post-copy fact.
        let b_store = TheoryStore::open(&bob.paths, &theory_id).unwrap();
        let (entries, malformed) = b_store.entries();
        assert!(
            malformed.is_empty(),
            "bob decrypts everything: {malformed:?}"
        );
        let spls: Vec<String> = entries
            .iter()
            .filter_map(|e| crate::core::envelope::parse_wire(&e.cbcl).ok())
            .filter_map(|a| match a {
                SpeechAct::Assert { spl, .. } => Some(spl),
                _ => None,
            })
            .collect();
        assert!(
            spls.iter().any(|s| s.contains("post-copy-fact")),
            "continuous sync carried the post-copy fact: {spls:?}"
        );
    }

    /// REQ-107: a peer whose transport key is not on the roster is refused
    /// before any corpus data flows.
    #[tokio::test]
    async fn responder_rejects_non_member() {
        let alice = node("alice");
        let a_store = TheoryStore::create(
            &alice.paths,
            &alice.ident,
            "t",
            1_784_000_000_000,
            "2026-07-11T00:00:00Z",
        )
        .unwrap();
        let theory_id = a_store.theory_id.clone();
        let mallory = node("mallory"); // never admitted

        let (mut sa, mut sb) = tokio::io::duplex(1 << 20);
        let a_paths = alice.paths.clone();
        let mallory_pk = transport::node_pk(&mallory.ident);
        let responder =
            tokio::spawn(async move { sync_as_responder(&mut sa, &mallory_pk, &a_paths).await });
        // Mallory dials as if a member.
        let m_paths = mallory.paths.clone();
        let m_theory = theory_id.clone();
        // Mallory has no local store for the theory, so drive the raw offer.
        let initiator = tokio::spawn(async move {
            use crate::p2p::sync_dialect::SyncMsg;
            let doc = loro::LoroDoc::new();
            let offer = SyncMsg::Offer {
                theory: m_theory,
                vv: doc.oplog_vv().encode(),
            };
            wire::write_frame(&mut sb, offer.to_cbcl().as_bytes())
                .await
                .ok();
            let _ = &m_paths;
        });
        let result = responder.await.unwrap();
        initiator.await.unwrap();
        assert!(result.is_err(), "a non-member sync must be refused");
    }
}
