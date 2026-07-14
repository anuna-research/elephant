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

fn view_bytes(entries: &[Entry]) -> String {
    let c = close(entries);
    serde_json::to_string(&vocab::view_json(&vocab::view(&c))).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Shuffled corpus ⇒ byte-identical vocab --json, LWW winners included.
    #[test]
    fn vocab_view_is_merge_order_independent(
        perm in Just((0..12usize).collect::<Vec<_>>()).prop_shuffle()
    ) {
        let base = corpus();
        let canonical = view_bytes(&canonicalise(base.clone()));
        let shuffled: Vec<Entry> = perm.iter().map(|&i| base[i].clone()).collect();
        let again = view_bytes(&canonicalise(shuffled));
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
