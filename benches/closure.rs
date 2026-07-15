//! TEST-025 / NFR-001: closure latency at 1 000 and 10 000 entries.
//!
//! Run with `cargo bench`. The NFR budget is closure ≤ 250 ms p95 at 1 000
//! entries (NFR-001) and functional (no blow-up beyond ×20) at 10 000
//! (NFR-003). These are reported by criterion; a CI gate can assert the
//! mean against the budget.

use criterion::{Criterion, criterion_group, criterion_main};
use elephant::core::closure;
use elephant::core::envelope::{Entry, Hlc, SpeechAct};
use std::hint::black_box;

fn corpus(n: usize) -> (Vec<Entry>, ed25519_dalek::VerifyingKey) {
    let key = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
    // BUG-002 fix: the DID and HLC node id must be the ones merge-time
    // validation derives from the signing key, or every entry quarantines
    // and the bench measures the rejection path instead of reasoning.
    let did = format!("did:crdt:{}", "09".repeat(32));
    let node = did_crdt::core::validate::node_id_from_pubkey(key.verifying_key().as_bytes());
    let did = did.as_str();
    let mut entries = Vec::with_capacity(n);
    // A realistic mix: a chain of facts feeding one gated conclusion, plus
    // independent facts. This exercises rule firing, not just fact lookup.
    for i in 0..n {
        let hlc = Hlc {
            wall_ms: 1_784_000_000_000 + i as u64,
            logical: 0,
            node_id: node,
        };
        let spl = if i == 0 {
            "(normally r-gate (and f-0 f-1 f-2) goal)".to_string()
        } else {
            format!("(given f-{i})")
        };
        let sid = Entry::sentence_id("bench", did, hlc);
        entries.push(Entry::create(
            "bench",
            hlc,
            did,
            &format!("{did}#key-0"),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl,
            },
            "2026-07-11T00:00:00Z",
            &key,
        ));
    }
    (entries, key.verifying_key())
}

/// SPEC-005 NFR-401 / TEST-408: the vocab view must complete within the
/// NFR-001 closure budget + 50 ms — measured here as the *incremental*
/// cost of `vocab::view` over an already-computed closure (family
/// resolution, docs join, provenance pass, witness join).
fn bench_vocab_view(c: &mut Criterion) {
    let mut group = c.benchmark_group("vocab_view");
    let n = 1_000usize;
    let (entries, vk) = corpus(n);
    let resolve = move |_: &str, _: &str| Some(vk);
    let closure = closure::close(
        &entries,
        "bench",
        "genesis",
        &resolve,
        "",
        1_784_000_100_000,
    )
    .unwrap();
    assert!(
        closure.quarantined.is_empty(),
        "bench corpus must be admitted (BUG-002)"
    );
    group.bench_function(format!("{n}_entries"), |b| {
        b.iter(|| {
            let v = elephant::core::vocab::view(black_box(&closure));
            black_box(v.rows.len())
        })
    });
    group.finish();
}

fn bench_closure(c: &mut Criterion) {
    let mut group = c.benchmark_group("closure");
    for &n in &[100usize, 1_000, 10_000] {
        let (entries, vk) = corpus(n);
        let resolve = move |_: &str, _: &str| Some(vk);
        group.bench_function(format!("{n}_entries"), |b| {
            b.iter(|| {
                let r = closure::close(
                    black_box(&entries),
                    "bench",
                    "genesis",
                    &resolve,
                    "",
                    1_784_000_100_000,
                )
                .unwrap();
                black_box(r.conclusions.len())
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_closure, bench_vocab_view);
criterion_main!(benches);
