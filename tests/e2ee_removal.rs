//! TEST-304/TEST-305 — the load-bearing E2EE properties (SPEC-004):
//!
//! 1. A newly admitted member receives the whole keybook and can read the
//!    entire corpus history (REQ-304) — an elephant never forgets.
//! 2. A removed member cannot decrypt anything sealed after rotation
//!    (REQ-305), even though they still hold every earlier key and the
//!    ciphertext bytes.

use elephant::core::envelope::{Entry, Hlc, SpeechAct};
use elephant::e2ee::keybook::Keybook;
use elephant::e2ee::seal::{self, OpenError};
use elephant::e2ee::{self, MlsIdentity};
use elephant::paths::Paths;

struct Peer {
    _dir: tempfile::TempDir,
    provider: e2ee::provider::DurableProvider,
    mls: MlsIdentity,
    did: String,
    signing_key: ed25519_dalek::SigningKey,
}

const THEORY: &str = "th-removal";

fn peer(name: &str) -> Peer {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: dir.path().to_path_buf(),
    };
    let ident = elephant::id::create(&paths, Some(name.into())).unwrap();
    let provider = e2ee::open_provider(&paths, THEORY).unwrap();
    let mls = MlsIdentity::for_theory(&ident, THEORY);
    Peer {
        _dir: dir,
        provider,
        mls,
        did: ident.did.to_string(),
        signing_key: ident.signing_key.clone(),
    }
}

fn entry_for(p: &Peer, n: u64, spl: &str) -> Entry {
    Entry::create(
        THEORY,
        Hlc {
            wall_ms: 1_784_000_000_000 + n,
            logical: 0,
            node_id: n,
        },
        &p.did,
        &format!("{}#key-0", p.did),
        &SpeechAct::Assert {
            sentence_id: format!("s-{n:016x}"),
            spl: spl.to_string(),
        },
        "2026-07-11T00:00:00Z",
        &p.signing_key,
    )
}

/// Full ceremony: create → seal → add member → keybook handover → member
/// reads history → remove member → rotate → member locked out.
#[test]
fn removed_member_cannot_read_after_rotation() {
    let alice = peer("alice"); // steward
    let bob = peer("bob");

    // ── epoch 0: alice creates the group and the keybook, seals an entry
    let mut ag = e2ee::create_group(&alice.provider, &alice.mls, THEORY).unwrap();
    let (mut alice_kb, k0) = Keybook::genesis();
    let early = entry_for(&alice, 1, "(given early-secret)");
    let sealed_early = seal::seal(&early, THEORY, 0, &k0).unwrap();

    // ── alice adds bob
    let (_bundle, kp_bytes) = e2ee::build_key_package(&bob.provider, &bob.mls).unwrap();
    let kp = e2ee::key_package_from_bytes(&alice.provider, &kp_bytes, &bob.did).unwrap();
    let (_commit, welcome) = e2ee::add_member(&alice.provider, &mut ag, &alice.mls, kp).unwrap();
    let mut bg = e2ee::join_from_welcome(&bob.provider, &welcome, &alice.did).unwrap();

    // ── REQ-304: alice sends the keybook as an MLS application message
    let kb_msg =
        e2ee::encrypt_app(&alice.provider, &mut ag, &alice.mls, &alice_kb.to_bytes()).unwrap();
    let mut bob_kb = Keybook {
        v: 1,
        keys: Default::default(),
        current: 0,
    };
    match e2ee::process_inbound(&bob.provider, &mut bg, &kb_msg, &alice.did).unwrap() {
        e2ee::Inbound::Application { sender_did, bytes } => {
            assert_eq!(sender_did, alice.did, "keybook must come from the steward");
            bob_kb.merge(&Keybook::from_bytes(&bytes).unwrap()).unwrap();
        }
        other => panic!("expected the keybook application message, got {other:?}"),
    }

    // Bob reads the pre-join history (the elephant remembers for him).
    let opened = seal::open(&sealed_early, THEORY, &|g| bob_kb.key(g)).unwrap();
    assert_eq!(opened, early, "admitted member must read pre-join history");

    // ── REQ-305: alice removes bob, rotates the data key
    let _commit = e2ee::remove_member(&alice.provider, &mut ag, &alice.mls, &bob.did).unwrap();
    assert_eq!(
        e2ee::member_dids(&ag),
        vec![alice.did.clone()],
        "bob is out of the MLS group"
    );
    let k1 = alice_kb.rotate();
    assert_eq!(alice_kb.current, 1);

    // Alice seals a new entry under the new generation.
    let late = entry_for(&alice, 2, "(given post-removal-secret)");
    let sealed_late = seal::seal(&late, THEORY, 1, &k1).unwrap();

    // Alice can still read everything.
    assert!(seal::open(&sealed_early, THEORY, &|g| alice_kb.key(g)).is_ok());
    assert!(seal::open(&sealed_late, THEORY, &|g| alice_kb.key(g)).is_ok());

    // ── the property: bob holds gen 0 and the ciphertext, and is locked out.
    assert_eq!(
        seal::open(&sealed_late, THEORY, &|g| bob_kb.key(g)).unwrap_err(),
        OpenError::UnknownGeneration(1),
        "removed member has no key for the post-rotation generation"
    );
    // Even if bob mislabels his old key as generation 1, the AEAD refuses:
    // the generation is bound into the AAD.
    let k0_only = bob_kb.key(0).unwrap();
    assert_eq!(
        seal::open(&sealed_late, THEORY, &|_| Some(k0_only)).unwrap_err(),
        OpenError::AeadFailed,
        "the old key must not open post-rotation ciphertext"
    );
    // And bob never receives the new keybook: rotation happened after the
    // MLS removal, so any keybook message alice now sends is unreadable to
    // him. (He is no longer in the group; there is no epoch he can process.)
    let post_kb =
        e2ee::encrypt_app(&alice.provider, &mut ag, &alice.mls, &alice_kb.to_bytes()).unwrap();
    let attempt = e2ee::process_inbound(&bob.provider, &mut bg, &post_kb, &alice.did);
    assert!(
        matches!(
            attempt,
            Err(_) | Ok(e2ee::Inbound::Buffered) | Ok(e2ee::Inbound::Ignored)
        ),
        "a removed member must not decrypt post-removal group messages"
    );

    // Bob's pre-removal read access to history is unaffected — the corpus is
    // append-only and he legitimately saw it. That is by design (ADR-302).
    assert!(seal::open(&sealed_early, THEORY, &|g| bob_kb.key(g)).is_ok());
}
