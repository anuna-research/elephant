//! End-to-end two-daemon loopback sync (SPEC-002 REQ-107/108 composed).
//!
//! Two real homes join a theory through the real ceremony, then both run the
//! daemon's actual continuous-sync loops (`p2p::sync::run_with_endpoint` —
//! accept + roster-gated dial) over live iroh QUIC on 127.0.0.1. No DHT, no
//! relay: peer addresses are injected via `MemoryLookup` exactly where
//! production resolves through the Mainline DHT, so nothing is published to
//! the public network.
//!
//! `#[ignore]`d for the same reason as `transport::rendezvous_dial_over_quic`:
//! live QUIC loopback dialing is not deterministic enough for CI. Run on a
//! real machine with:
//!
//!     cargo test --test sync_live -- --ignored

use std::sync::Arc;
use std::time::Duration;

use elephant::paths::Paths;
use elephant::store::TheoryStore;
use elephant::{id, p2p};
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr, TransportAddr};
use tokio::io::duplex;

struct Machine {
    _dir: tempfile::TempDir,
    paths: Paths,
    ident: id::Identity,
}

fn machine(name: &str) -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: dir.path().to_path_buf(),
    };
    let ident = id::create(&paths, Some(name.into())).unwrap();
    Machine {
        _dir: dir,
        paths,
        ident,
    }
}

fn ctx(m: &Machine, theory: &str) -> elephant::cli::Ctx {
    elephant::cli::Ctx {
        paths: m.paths.clone(),
        json: true,
        theory: Some(theory.to_string()),
        at: None,
    }
}

/// The corpus SPL statements a machine can currently decrypt and parse.
fn spls(m: &Machine, theory: &str) -> Vec<String> {
    let store = TheoryStore::open(&m.paths, theory).unwrap();
    let (entries, _) = store.entries();
    entries
        .iter()
        .filter_map(|e| elephant::core::envelope::parse_wire(&e.cbcl).ok())
        .filter_map(|a| match a {
            elephant::core::envelope::SpeechAct::Assert { spl, .. } => Some(spl),
            _ => None,
        })
        .collect()
}

/// Poll until `m` holds a statement containing `needle`, or panic after 60s.
async fn wait_for_fact(m: &Machine, theory: &str, needle: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if spls(m, theory).iter().any(|s| s.contains(needle)) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "'{needle}' did not sync over within 60s; corpus: {:?}",
            spls(m, theory)
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// A dialable loopback address for a bound endpoint: explicit ip, no lookup.
fn loopback_addr(ep: &Endpoint) -> EndpointAddr {
    let port = ep
        .bound_sockets()
        .into_iter()
        .find(|s| s.is_ipv4())
        .expect("an ipv4 socket")
        .port();
    EndpointAddr::from_parts(
        ep.id(),
        [TransportAddr::Ip(
            format!("127.0.0.1:{port}").parse().unwrap(),
        )],
    )
}

/// Bind a machine's durable sync endpoint on loopback with an injectable
/// address book — the same transport key and ALPN as
/// `transport::sync_endpoint`, minus the DHT.
async fn sync_endpoint_loopback(m: &Machine) -> (Endpoint, MemoryLookup) {
    let lookup = MemoryLookup::new();
    let ep = Endpoint::builder(presets::Minimal)
        .secret_key(p2p::transport::transport_secret(&m.ident))
        .alpns(vec![p2p::wire::ALPN_SYNC.to_vec()])
        .address_lookup(lookup.clone())
        .bind()
        .await
        .expect("bind sync endpoint");
    (ep, lookup)
}

/// Alice creates a theory and admits Bob through the real join ceremony
/// (over a duplex — the rendezvous transport has its own ignored test); then
/// both machines run the daemon's continuous-sync loops over live QUIC
/// loopback, and facts written on either side converge to the other with no
/// further ceremony (REQ-108), gated by the roster (REQ-107).
#[tokio::test]
#[ignore = "live QUIC loopback; run manually: cargo test --test sync_live -- --ignored"]
async fn two_daemons_converge_over_loopback() {
    let alice = machine("alice");
    let bob = machine("bob");

    // ── setup: theory + real join ceremony (roster records real keys) ──
    let store = TheoryStore::create(
        &alice.paths,
        &alice.ident,
        "release",
        1_784_000_000_000,
        "2026-07-11T00:00:00Z",
    )
    .unwrap();
    let theory_id = store.theory_id.clone();
    drop(store);

    let invite = p2p::invite::Invite::generate();
    let password = invite.password();
    let bob_node_pk = p2p::transport::node_pk(&bob.ident);

    let (mut sa, mut sb) = duplex(4 * 1024 * 1024);
    let inviter = tokio::spawn({
        let (paths, tid, password) = (alice.paths.clone(), theory_id.clone(), password.clone());
        async move {
            let ident = id::load(&paths).unwrap();
            p2p::join::inviter_side(&mut sa, &paths, &ident, &tid, "rdv:test", &password, "hint")
                .await
        }
    });
    let joiner = tokio::spawn({
        let (paths, password) = (bob.paths.clone(), password.clone());
        async move {
            let ident = id::load(&paths).unwrap();
            p2p::join::joiner_side(
                &mut sb,
                &paths,
                &ident,
                "rdv:test",
                &password,
                &bob_node_pk,
                None,
                Some("release"),
            )
            .await
        }
    });
    inviter.await.unwrap().expect("inviter side");
    joiner.await.unwrap().expect("joiner side");

    // ── the two "daemons": production sync loops over loopback QUIC ──
    let (ep_a, lookup_a) = sync_endpoint_loopback(&alice).await;
    let (ep_b, lookup_b) = sync_endpoint_loopback(&bob).await;
    lookup_a.add_endpoint_info(loopback_addr(&ep_b));
    lookup_b.add_endpoint_info(loopback_addr(&ep_a));

    let shutdown = Arc::new(tokio::sync::Notify::new());
    let interval = Duration::from_secs(1);
    let daemon_a = tokio::spawn(p2p::sync::run_with_endpoint(
        ep_a,
        alice.paths.clone(),
        Arc::new(id::load(&alice.paths).unwrap()),
        shutdown.clone(),
        interval,
        None,
        None,
    ));
    let daemon_b = tokio::spawn(p2p::sync::run_with_endpoint(
        ep_b,
        bob.paths.clone(),
        Arc::new(id::load(&bob.paths).unwrap()),
        shutdown.clone(),
        interval,
        None,
        None,
    ));

    // ── steward → member: only continuous sync can carry this ──
    elephant::cli::append_asserts(&ctx(&alice, &theory_id), &["(given qa-signed)".to_string()])
        .unwrap();
    wait_for_fact(&bob, &theory_id, "qa-signed").await;

    // ── member → steward: the reverse direction, same loops ──
    elephant::cli::append_asserts(
        &ctx(&bob, &theory_id),
        &["(given legal-signed)".to_string()],
    )
    .unwrap();
    wait_for_fact(&alice, &theory_id, "legal-signed").await;

    // ── both replicas converged (REQ-108) ──
    let a = TheoryStore::open(&alice.paths, &theory_id).unwrap();
    let b = TheoryStore::open(&bob.paths, &theory_id).unwrap();
    assert_eq!(
        a.doc().oplog_vv(),
        b.doc().oplog_vv(),
        "version vectors equal after steady-state sync"
    );

    shutdown.notify_waiters();
    let _ = daemon_a.await;
    let _ = daemon_b.await;
}
