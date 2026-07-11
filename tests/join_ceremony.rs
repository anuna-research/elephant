//! TEST-104/105/108/111 — the whole join ceremony over an in-memory duplex.
//! SPAKE2 → confirmation → MLS Welcome → sealed introduction → keybook →
//! roster fact → corpus sync, exactly as it runs over an iroh bi-stream.

use elephant::paths::Paths;
use elephant::store::TheoryStore;
use elephant::{id, p2p};
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

/// Alice creates a theory with a fact, invites Bob; Bob joins and ends up
/// with the same corpus, the same conclusions, and a roster containing both.
#[tokio::test]
async fn happy_path_join_and_sync() {
    let alice = machine("alice");
    let bob = machine("bob");

    let store = TheoryStore::create(
        &alice.paths,
        &alice.ident,
        "release",
        1_784_000_000_000,
        "2026-07-11T00:00:00Z",
    )
    .unwrap();
    let theory_id = store.theory_id.clone();

    // Alice asserts a fact before Bob exists — Bob must be able to read it.
    let ctx = elephant::cli::Ctx {
        paths: alice.paths.clone(),
        json: true,
        theory: Some(theory_id.clone()),
        at: None,
    };
    elephant::cli::append_asserts(&ctx, &["(given qa-signed)".to_string()]).unwrap();
    drop(store);

    let invite = p2p::invite::Invite::generate();
    let password = invite.password();
    let alice_pk = p2p::transport::node_pk(&alice.ident);

    let (mut sa, mut sb) = duplex(4 * 1024 * 1024);
    let (a_paths, a_ident, tid) = (alice.paths.clone(), alice.ident, theory_id.clone());
    let inviter = tokio::spawn({
        let password = password.clone();
        async move {
            p2p::join::inviter_side(
                &mut sa,
                &a_paths,
                &a_ident,
                &tid,
                "rdv:1234",
                &password,
                "endpoint-hint",
            )
            .await
        }
    });
    let (b_paths, b_ident, tid2) = (bob.paths.clone(), bob.ident, theory_id.clone());
    let joiner = tokio::spawn({
        let password = password.clone();
        async move {
            let _ = &tid2;
            p2p::join::joiner_side(
                &mut sb,
                &b_paths,
                &b_ident,
                "rdv:1234",
                &password,
                "bob-node-pk",
                None,
                Some("release"),
            )
            .await
        }
    });

    let joined_did = inviter.await.unwrap().expect("inviter side");
    let joined_theory = joiner.await.unwrap().expect("joiner side");
    assert_eq!(joined_theory, theory_id);
    assert!(joined_did.starts_with("did:crdt:"));

    // Bob's replica decrypts and derives Alice's pre-join fact (REQ-304).
    let bob_store = TheoryStore::open(&bob.paths, &theory_id).unwrap();
    let (entries, malformed) = bob_store.entries();
    assert!(
        malformed.is_empty(),
        "bob must open every sealed entry: {malformed:?}"
    );
    assert!(
        entries.len() >= 3,
        "genesis + qa-signed + member fact, got {}",
        entries.len()
    );

    let bob_ctx = elephant::cli::Ctx {
        paths: bob.paths.clone(),
        json: true,
        theory: Some(theory_id.clone()),
        at: None,
    };
    let view = elephant::queries::view(&bob_ctx).unwrap();
    let holds: Vec<String> = elephant::queries::conclusions_positive(&view)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert!(
        holds.iter().any(|l| l == "qa-signed"),
        "bob derives alice's pre-join fact: {holds:?}"
    );
    // REQ-105: the roster is a closure-derived member fact naming bob.
    assert!(
        holds.iter().any(|l| l.starts_with("member")),
        "roster fact present: {holds:?}"
    );
    // REQ-107 / BUG-002 regression: the roster must name BOTH transport keys
    // — the steward's included — or steady-state sync between steward and a
    // single member is refused in both directions (see tests/sync_live.rs).
    let roster = p2p::sync::roster_node_pks(&bob.paths, &theory_id).unwrap();
    assert!(
        roster.contains(&alice_pk),
        "steward's transport key on the roster: {roster:?}"
    );
    assert!(
        roster.contains("bob-node-pk"),
        "joiner's transport key on the roster: {roster:?}"
    );

    // REQ-108: both replicas converged.
    let alice_store = TheoryStore::open(&alice.paths, &theory_id).unwrap();
    assert_eq!(
        alice_store.doc().oplog_vv(),
        bob_store.doc().oplog_vv(),
        "version vectors must be equal after sync"
    );
}

/// REQ-111: a wrong code fails opaquely and leaves the joiner with no
/// partial theory state.
#[tokio::test]
async fn wrong_code_fails_closed_with_no_partial_state() {
    let alice = machine("alice");
    let bob = machine("bob");
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

    let (mut sa, mut sb) = duplex(1024 * 1024);
    let (a_paths, a_ident, tid) = (alice.paths.clone(), alice.ident, theory_id.clone());
    let inviter = tokio::spawn(async move {
        p2p::join::inviter_side(
            &mut sa,
            &a_paths,
            &a_ident,
            &tid,
            "rdv:1",
            "abandon-ability",
            "hint",
        )
        .await
    });
    let (b_paths, b_ident, tid2) = (bob.paths.clone(), bob.ident, theory_id.clone());
    let joiner = tokio::spawn(async move {
        let _ = &tid2;
        p2p::join::joiner_side(
            &mut sb,
            &b_paths,
            &b_ident,
            "rdv:1",
            "zebra-zone",
            "pk",
            None,
            None,
        )
        .await
    });

    let ir = inviter.await.unwrap();
    let jr = joiner.await.unwrap();
    assert!(ir.is_err() || jr.is_err(), "wrong password must fail");
    for e in [ir.err(), jr.err()].into_iter().flatten() {
        assert!(
            e.to_string().contains("auth-failed") || e.to_string().contains("transport"),
            "failure must be opaque, got: {e}"
        );
    }
    // No theory materialised on bob's machine.
    assert!(
        TheoryStore::open(&bob.paths, &theory_id).is_err(),
        "a failed join must leave no theory behind"
    );
}

/// A mismatched SPAKE2 hint (different routing number) cannot complete —
/// the hint is bound into the SPAKE2 identity.
#[tokio::test]
async fn spake_hint_binding_enforced() {
    let alice = machine("alice");
    let bob = machine("bob");
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

    let (mut sa, mut sb) = duplex(1024 * 1024);
    let (a_paths, a_ident, tid) = (alice.paths.clone(), alice.ident, theory_id.clone());
    let inviter = tokio::spawn(async move {
        p2p::join::inviter_side(
            &mut sa,
            &a_paths,
            &a_ident,
            &tid,
            "rdv:1234",
            "abandon-ability",
            "h",
        )
        .await
    });
    let (b_paths, b_ident) = (bob.paths.clone(), bob.ident);
    let joiner = tokio::spawn(async move {
        // Bob uses the right words but a different routing hint.
        p2p::join::joiner_side(
            &mut sb,
            &b_paths,
            &b_ident,
            "rdv:9999",
            "abandon-ability",
            "pk",
            None,
            None,
        )
        .await
    });
    let ir = inviter.await.unwrap();
    let jr = joiner.await.unwrap();
    assert!(ir.is_err() || jr.is_err(), "theory hint must be bound");
}
