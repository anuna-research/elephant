//! SPEC-005 TEST-408: the vocabulary view is a pure projection — the same
//! signed Entry set, merged in any order, yields byte-identical
//! `vocab --json` rows, including per-key LWW documentation winners
//! (ADR-403; extends SPEC-001 REQ-021 to the vocab view).

use elephant::core::closure::{self, Closure};
use elephant::core::envelope::{Entry, Hlc, SpeechAct};
use elephant::core::vocab;
use proptest::prelude::*;

fn key(seed: u8) -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
}

fn node(seed: u8) -> u64 {
    did_crdt::core::validate::node_id_from_pubkey(key(seed).verifying_key().as_bytes())
}

fn did(seed: u8) -> String {
    format!("did:crdt:{}", format!("{seed:02x}").repeat(32))
}

/// A corpus with LWW conflicts (two signers documenting the same families),
/// rules, facts, commitments-adjacent shapes, and a task declaration —
/// each statement pinned to a FIXED HLC so the canonical (hlc, signer)
/// order is a property of the set, not of insertion order.
fn corpus() -> Vec<Entry> {
    let stmts: Vec<(u8, &str)> = vec![
        (1, "(given task-m1)"),
        (
            1,
            "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        ),
        (2, "(given (review-approved m1))"),
        (
            1,
            "(meta (predicate ci-green 1) (description \"CI green by one\") (kind evidence))",
        ),
        (
            2,
            "(meta (predicate ci-green 1) (description \"CI green by two\") (asserter \"role:ci\"))",
        ),
        (
            1,
            "(meta deploy-thing (description \"first\") (kind bogus))",
        ),
        (2, "(meta deploy-thing (description \"second\"))"),
        (2, "(given deploy-thing)"),
        (1, "(given stale-note-m1)"),
        (1, "(given (p x))"),
        (2, "(given p/1)"),
        (1, "(normally r-flat ci-green-m1 verified-m1)"),
    ];
    stmts
        .into_iter()
        .enumerate()
        .map(|(i, (seed, spl))| {
            let hlc = Hlc {
                wall_ms: 1_784_000_000_000 + i as u64,
                logical: 0,
                node_id: node(seed),
            };
            let sid = Entry::sentence_id("th-x", &did(seed), hlc);
            Entry::create(
                "th-x",
                hlc,
                &did(seed),
                &format!("{}#key-0", did(seed)),
                &SpeechAct::Assert {
                    sentence_id: sid,
                    spl: spl.to_string(),
                },
                "2026-07-11T00:00:00Z",
                &key(seed),
            )
        })
        .collect()
}

/// The store's read-side canonicalisation (SPEC-001 CON-002 total order):
/// merge order in, (hlc, signer, sig) out.
fn canonicalise(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| {
        a.hlc
            .cmp(&b.hlc)
            .then_with(|| a.signer.cmp(&b.signer))
            .then_with(|| a.sig.cmp(&b.sig))
    });
    entries
}

fn close(entries: &[Entry]) -> Closure {
    let resolve = |d: &str, _k: &str| -> Option<ed25519_dalek::VerifyingKey> {
        (1u8..=9)
            .find(|s| did(*s) == d)
            .map(|s| key(s).verifying_key())
    };
    closure::close(entries, "th-x", "genesis", &resolve, "", 1_784_000_000_000).unwrap()
}

/// Append the single-signer statement library to a REAL store in the given
/// insertion order (fixed per-statement HLCs, so the canonical order is a
/// property of the set), read it back through `TheoryStore::entries()`
/// (the store's own sort is the convergence mechanism under test), close,
/// and render the view. This is the actual merge path a replica takes.
fn store_view_bytes(
    paths: &elephant::paths::Paths,
    ident: &elephant::id::Identity,
    theory_name: &str,
    order: &[usize],
) -> String {
    use elephant::store::TheoryStore;
    let stmts: Vec<&str> = vec![
        "(given task-m1)",
        "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        "(given (review-approved m1))",
        "(meta (predicate ci-green 1) (description \"CI green v1\") (kind evidence))",
        "(meta (predicate ci-green 1) (description \"CI green v2\"))",
        "(meta deploy-thing (description \"first\") (kind bogus))",
        "(meta deploy-thing (description \"second\"))",
        "(given deploy-thing)",
        "(given stale-note-m1)",
        "(given (p x))",
        "(given p/1)",
        "(normally r-flat ci-green-m1 verified-m1)",
    ];
    let store = TheoryStore::create(
        paths,
        ident,
        theory_name,
        1_783_999_999_000,
        "2026-07-11T00:00:00Z",
    )
    .unwrap();
    store.bind_identity(ident).unwrap();
    for &i in order {
        let hlc = elephant::core::envelope::Hlc {
            wall_ms: 1_784_000_000_000 + i as u64, // fixed per statement
            logical: 0,
            node_id: did_crdt::core::validate::node_id_from_pubkey(
                ident.signing_key.verifying_key().as_bytes(),
            ),
        };
        let sid = Entry::sentence_id(&store.theory_id, ident.did.as_str(), hlc);
        let e = Entry::create(
            &store.theory_id,
            hlc,
            ident.did.as_str(),
            &format!("{}#key-0", ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl: stmts[i].to_string(),
            },
            "2026-07-11T00:00:00Z",
            &ident.signing_key,
        );
        store.append(&e).unwrap();
    }
    let (entries, _) = store.entries();
    let resolve = store.key_resolver();
    let c = closure::close(
        &entries,
        &store.theory_id,
        elephant::store::GENESIS_THEORY,
        &resolve,
        "",
        1_784_000_100_000,
    )
    .unwrap();
    assert!(c.quarantined.is_empty(), "fixture must be admitted");
    serde_json::to_string(&vocab::view_json(&vocab::view(&c))).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]

    /// Shuffled INSERTION order into the real store ⇒ byte-identical
    /// vocab --json (the store's canonical sort + the view's determinism,
    /// LWW winners included).
    #[test]
    fn vocab_view_is_merge_order_independent(
        perm in Just((0..12usize).collect::<Vec<_>>()).prop_shuffle()
    ) {
        // One identity for both runs — the documenter DID is part of the
        // output; only the insertion order may vary.
        let dir = tempfile::tempdir().unwrap();
        let paths = elephant::paths::Paths { home: dir.path().to_path_buf() };
        let ident = elephant::id::create(&paths, Some("p".into())).unwrap();
        let canonical = store_view_bytes(&paths, &ident, "ta", &(0..12).collect::<Vec<_>>());
        let again = store_view_bytes(&paths, &ident, "tb", &perm);
        prop_assert_eq!(canonical, again, "vocab view diverged across merge orders");
    }
}

/// The LWW winners in the fixture are pinned: signer 2's later writes win
/// per key; signer 1's conforming kind survives its own malformed key row.
#[test]
fn lww_winners_pinned() {
    let c = close(&canonicalise(corpus()));
    let v = vocab::view(&c);
    let ci = v
        .rows
        .iter()
        .find(|r| r.family.rendered() == "ci-green/1" && r.family.kind() == "predicate")
        .unwrap();
    assert_eq!(ci.doc.description.as_deref(), Some("CI green by two"));
    // kind was written only by signer 1; asserter only by signer 2 —
    // per-key merge keeps both (ADR-403).
    assert_eq!(ci.doc.kind.as_deref(), Some("evidence"));
    assert_eq!(ci.doc.asserter.as_deref(), Some("role:ci"));
    assert_eq!(ci.doc.documenter.as_deref(), Some(did(2).as_str()));
    assert!(ci.doc.redefined, "two distinct signers wrote descriptions");

    let dt = v
        .rows
        .iter()
        .find(|r| r.family.rendered() == "deploy-thing")
        .unwrap();
    assert_eq!(dt.doc.description.as_deref(), Some("second"));
    // signer 1's bogus kind is the winning (only) kind write → malformed.
    assert_eq!(dt.doc.malformed, vec!["kind".to_string()]);
}
