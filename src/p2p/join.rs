//! The join ceremony choreography (SPEC-002 REQ-104/105/110/111).
//!
//! Written against `AsyncRead + AsyncWrite`, so the whole choreography —
//! SPAKE2, key confirmation, sealed introduction, MLS Welcome, keybook,
//! corpus sync — runs identically over an iroh QUIC bi-stream and over an
//! in-memory duplex in tests. Failures after code entry are remotely opaque
//! (`auth-failed`).
//!
//! Wire order (each side's step numbers match):
//! ```text
//!   J → I  SPAKE2 msg   (the joiner speaks first: over QUIC the inviter's
//!                        accept_bi only resolves once stream data arrives)
//!   I → J  SPAKE2 msg
//!   confirmation MACs  (both)
//!   I → J  sealed introduction   (theory id, steward DID doc, endpoint)
//!   J → I  joiner hello          (DID, DID doc, node key, MLS KeyPackage)
//!   I → J  MLS Welcome           (built for THIS joiner's package)
//!   I → J  keybook               (MLS application message)
//!   sync session                 (both)
//! ```
//! The introduction precedes the joiner's hello because the joiner needs
//! the theory id to derive its per-theory MLS leaf key before it can build
//! a KeyPackage. SPAKE2 is bound to the invite's public routing hint, not
//! the theory id — the joiner does not know the theory id yet.

use super::pake::{self, Handshake, Introduction, Side};
use super::wire::{read_frame, sync_session, write_frame};
use crate::core::envelope::{Entry, SpeechAct};
use crate::errors::{AppError, AppResult};
use crate::id::Identity;
use crate::paths::Paths;
use crate::store::TheoryStore;
use tokio::io::{AsyncRead, AsyncWrite};

/// What the inviter (steward) runs once a joiner connects to the rendezvous.
///
/// `spake_hint` is bound into the SPAKE2 identity — the invite's public
/// routing component, shared by both sides via the code.
#[allow(clippy::too_many_arguments)]
pub async fn inviter_side<S>(
    stream: &mut S,
    paths: &Paths,
    ident: &Identity,
    theory_id: &str,
    spake_hint: &str,
    password: &str,
    endpoint_hint: &str,
) -> AppResult<String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 1–2. SPAKE2 + key confirmation BEFORE any payload (REQ-104). The
    //    joiner's message comes first: it is what opens the QUIC bi-stream.
    // Step tracing is local-only diagnostics: the remote peer still sees
    // every failure as the opaque `auth-failed` (REQ-104).
    let (hs, ours) = Handshake::start(password, spake_hint, Side::Inviter);
    let theirs = read_frame(stream).await?;
    tracing::debug!(len = theirs.len(), "inviter: joiner SPAKE2 msg received");
    write_frame(stream, &ours).await?;
    let (keys, confirm) = hs.finish(&theirs)?;
    tracing::debug!("inviter: SPAKE2 finished");
    write_frame(stream, &confirm.ours(&keys)).await?;
    let peer_mac = read_frame(stream).await?;
    confirm.verify_peer(&keys, &peer_mac)?;
    tracing::debug!("inviter: key confirmation verified");

    // 3. Sealed introduction — the joiner learns the theory id (and can now
    //    derive its per-theory MLS leaf key).
    let store = TheoryStore::open(paths, theory_id)?;
    let intro = Introduction {
        v: pake::INTRO_VERSION,
        theory_id: theory_id.to_string(),
        alias: store.meta.alias.clone(),
        steward_did: ident.did.clone(),
        steward_did_doc: ident
            .document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did doc: {e}")))?,
        endpoint: endpoint_hint.to_string(),
    };
    write_frame(stream, &pake::seal_intro(&intro, &keys)?).await?;
    tracing::debug!("inviter: sealed introduction sent");

    // 4. Joiner hello: DID + MLS KeyPackage (built for this theory).
    let hello = read_frame(stream).await?;
    let hello: JoinerHello = serde_json::from_slice(&hello)
        .map_err(|e| AppError::Transport(format!("bad joiner hello: {e}")))?;
    tracing::debug!(did = %hello.did, v = hello.v, "inviter: joiner hello received");
    if hello.v != JOINER_HELLO_VERSION {
        tracing::debug!("inviter: joiner hello version mismatch");
        return Err(AppError::Signature("auth-failed".into()));
    }

    // 4a. Proof of possession (REQ-104): the joiner must control the private
    //     key behind the DID it claims, and `did_doc` must be that DID's
    //     document. Without this a joiner could enrol under any DID — the
    //     steward's included — and later be authorised by identity string.
    {
        use ed25519_dalek::Verifier as _;
        let sig_bytes: [u8; 64] = hello
            .pop
            .as_slice()
            .try_into()
            .map_err(|_| AppError::Signature("auth-failed".into()))?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        let msg = pop_message(theory_id, &hello.key_package);
        let keys = crate::id::verifying_keys_for(&hello.did_doc, hello.did.as_str());
        if !keys.iter().any(|vk| vk.verify(&msg, &sig).is_ok()) {
            // Distinguish "the doc resolved no keys" (format or library skew)
            // from "keys resolved but the signature does not verify".
            tracing::debug!(
                resolved_keys = keys.len(),
                "inviter: proof-of-possession failed"
            );
            return Err(AppError::Signature("auth-failed".into()));
        }
        tracing::debug!("inviter: proof-of-possession verified");
    }

    // 5. MLS Add (steward-only commit) → Welcome; send it framed.
    let provider = crate::e2ee::open_provider(paths, theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, theory_id);
    let mut group = crate::e2ee::load_group(&provider, theory_id)?
        .ok_or_else(|| AppError::Config("no MLS group for this theory".into()))?;
    // Reject a DID that is already a group member — in particular the
    // steward's own DID. Otherwise a second leaf could carry the steward
    // credential; duplicate identities also break the roster's key resolver
    // (SPEC-004 REQ-302 / ADR-303).
    if crate::e2ee::member_dids(&group)
        .iter()
        .any(|d| d == hello.did.as_str())
    {
        tracing::debug!("inviter: DID is already a group member");
        return Err(AppError::Signature("auth-failed".into()));
    }
    let kp =
        crate::e2ee::key_package_from_bytes(&provider, &hello.key_package, hello.did.as_str())?;
    let (commit, welcome) = crate::e2ee::add_member(&provider, &mut group, &mls_ident, kp)?;
    write_frame(stream, &welcome).await?;
    tracing::debug!("inviter: MLS welcome sent");
    // Publish the Add commit to the `mls` lane so members other than this
    // joiner advance to the new epoch when they next sync (REQ-108/REQ-306);
    // without it they could not process a later removal commit. `add_member`
    // recorded the commit in the durable outbox before advancing our epoch;
    // clear it once the publish lands (recovery republishes on crash).
    store.push_mls(&[commit])?;
    crate::e2ee::clear_outbox(paths, theory_id)?;

    // 6. Keybook, encrypted to the new MLS epoch (REQ-304): grants history.
    let keybook = store
        .keybook()
        .ok_or_else(|| AppError::Config("no keybook for this theory".into()))?;
    let kb_msg = crate::e2ee::encrypt_app(&provider, &mut group, &mls_ident, &keybook.to_bytes())?;
    write_frame(stream, &kb_msg).await?;

    // 7. Membership facts into the corpus (REQ-105) — signed, auditable, the
    //    closure-derived roster. The steward's OWN transport key must be on
    //    the roster too (REQ-107): the member's daemon admits — and dials —
    //    only peers the closure names, so without it steward↔member
    //    steady-state sync is refused in both directions (BUG-002). Genesis
    //    does not record it (a solo theory has no sync peers); the first
    //    admission adds it, deduplicated on later invites.
    store.bind_identity(ident)?;
    let my_fact = format!(
        "(given (member \"{}\" \"{}\"))",
        ident.did.as_str(),
        super::transport::node_pk(ident)
    );
    let have_my_fact = {
        let (entries, _) = store.entries();
        entries
            .iter()
            .filter_map(|e| crate::core::envelope::parse_wire(&e.cbcl).ok())
            .any(|a| matches!(&a, SpeechAct::Assert { spl, .. } if *spl == my_fact))
    };
    // HLC allocation and append under one clock guard: a concurrent
    // same-identity writer must not sign the same (wall, logical).
    let _clock = store.lock_clock()?;
    let (wall_ms, ts) = crate::cli::now_pair();
    let mut hlc = store.tick(ident, wall_ms);
    let mut entries = Vec::new();
    let mut push_member_fact = |spl: String, hlc: crate::core::envelope::Hlc| {
        let sid = Entry::sentence_id(theory_id, ident.did.as_str(), hlc);
        entries.push(Entry::create(
            theory_id,
            hlc,
            ident.did.as_str(),
            &format!("{}#key-0", ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl,
            },
            &ts,
            &ident.signing_key,
        ));
    };
    if !have_my_fact {
        push_member_fact(my_fact, hlc);
        hlc.logical += 1;
    }
    push_member_fact(
        format!("(given (member \"{}\" \"{}\"))", hello.did, hello.node_pk),
        hlc,
    );
    store.append_batch(&entries)?;
    drop(_clock);

    // Persist the joiner's DID document for offline signature verification.
    // The filename is the DID's method-specific id (64 hex chars, enforced by
    // the `Did` type) — never a raw, peer-chosen string (REQ-104).
    let member_dir = paths.theory_dir(theory_id).join("members");
    std::fs::create_dir_all(&member_dir)?;
    std::fs::write(
        member_dir.join(format!("{}.json", hello.did.method_specific_id())),
        &hello.did_doc,
    )?;

    // 8. Corpus sync: hand over the (sealed) history.
    tracing::debug!("inviter: starting corpus sync");
    sync_session(stream, theory_id, store.doc()).await?;
    Ok(hello.did.to_string())
}

/// What the joiner runs after resolving the rendezvous.
///
/// On any failure the joiner is left with no partial theory state (REQ-104).
/// `spake_hint` is the invite's public routing component; `expected_theory`
/// pins which theory the joiner intends to join (defence against a
/// rendezvous that offers a different theory than advertised).
#[allow(clippy::too_many_arguments)] // each is a distinct ceremony input;
// a params struct would only relocate the list (SPEC-002 REQ-104)
pub async fn joiner_side<S>(
    stream: &mut S,
    paths: &Paths,
    ident: &Identity,
    spake_hint: &str,
    password: &str,
    node_pk: &str,
    expected_theory: Option<&str>,
    alias_override: Option<&str>,
) -> AppResult<String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 1–2. SPAKE2 + confirmation. We write first: sending our SPAKE2 message
    //    is what makes the freshly-opened bi-stream visible to the inviter.
    let (hs, ours) = Handshake::start(password, spake_hint, Side::Joiner);
    write_frame(stream, &ours).await?;
    let theirs = read_frame(stream).await?;
    tracing::debug!(len = theirs.len(), "joiner: inviter SPAKE2 msg received");
    let (keys, confirm) = hs.finish(&theirs)?;
    let peer_mac = read_frame(stream).await?;
    confirm.verify_peer(&keys, &peer_mac)?;
    write_frame(stream, &confirm.ours(&keys)).await?;
    tracing::debug!("joiner: key confirmation verified");

    // 3. Sealed introduction → learn the theory id and steward DID.
    let intro = pake::open_intro(&read_frame(stream).await?, &keys)?;
    tracing::debug!(theory = %intro.theory_id, "joiner: sealed introduction opened");
    if let Some(want) = expected_theory {
        if want != intro.theory_id {
            return Err(AppError::Signature("auth-failed".into()));
        }
    }
    let theory_id = intro.theory_id.clone();

    // A previous join of this theory that died before the corpus sync
    // completed leaves an adopted shell: MLS group and keybook on disk but
    // zero corpus entries (the sync is the ceremony's final step, and no
    // create or completed join produces an entry-less replica — the theory id
    // is the hash of its genesis entry). Re-joining must not trip over that
    // wreckage (GroupAlreadyExists): clear the shell and adopt fresh. A
    // replica that holds entries — or that cannot be read at all — is never
    // touched. Doing this before the hello also spares the inviter a doomed
    // MLS add.
    let dir = paths.theory_dir(&theory_id);
    if dir.exists()
        && TheoryStore::open(paths, &theory_id)
            .map(|s| s.entries().0.is_empty())
            .unwrap_or(false)
    {
        tracing::debug!(theory = %theory_id, "joiner: clearing half-joined shell (no corpus entries)");
        std::fs::remove_dir_all(&dir)?;
    }

    // 4. Now that we know the theory, derive the leaf key and offer a
    //    KeyPackage bound to our DID.
    let provider = crate::e2ee::open_provider(paths, &theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, &theory_id);
    let (_bundle, key_package) = crate::e2ee::build_key_package(&provider, &mls_ident)?;
    // Prove we control the DID we claim (REQ-104): sign the enrolment with the
    // identity key, so the inviter can verify it against our DID document.
    use ed25519_dalek::Signer as _;
    let pop = ident
        .signing_key
        .sign(&pop_message(&theory_id, &key_package))
        .to_bytes()
        .to_vec();
    let hello = JoinerHello {
        v: JOINER_HELLO_VERSION,
        did: ident.did.clone(),
        did_doc: ident
            .document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did doc: {e}")))?,
        node_pk: node_pk.to_string(),
        key_package,
        pop,
    };
    write_frame(
        stream,
        &serde_json::to_vec(&hello).map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .await?;

    // 5. MLS Welcome → join the group, verifying the steward is the DID from
    //    the authenticated introduction (REQ-302).
    let welcome = read_frame(stream).await?;
    let mut group =
        crate::e2ee::join_from_welcome(&provider, &welcome, intro.steward_did.as_str())?;

    // 6. Keybook from the MLS application message.
    let kb_msg = read_frame(stream).await?;
    let keybook = match crate::e2ee::process_inbound(
        &provider,
        &mut group,
        &kb_msg,
        intro.steward_did.as_str(),
    )? {
        crate::e2ee::Inbound::Application { sender_did, bytes }
            if sender_did == intro.steward_did.as_str() =>
        {
            crate::e2ee::keybook::Keybook::from_bytes(&bytes)?
        }
        _ => return Err(AppError::Signature("auth-failed".into())),
    };

    // 7. Materialise the theory locally — only now do we touch the store, so
    //    a failure above leaves nothing behind.
    let alias = alias_override.unwrap_or(&intro.alias);
    let store = TheoryStore::adopt(
        paths,
        ident,
        &theory_id,
        alias,
        keybook,
        intro.steward_did.as_str(),
        &intro.steward_did_doc,
    )?;

    // 8. Pull the corpus.
    sync_session(stream, &theory_id, store.doc()).await?;
    store.flush_public()?;
    Ok(theory_id)
}

pub const JOINER_HELLO_VERSION: u16 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinerHello {
    pub v: u16,
    /// The joiner's DID. The `Did` type validates the `did:crdt:<64-hex>`
    /// shape on deserialisation, so the inviter can safely use its
    /// method-specific id as a filename (REQ-104).
    pub did: did_crdt::Did,
    #[serde(with = "super::pake::b64v_pub")]
    pub did_doc: Vec<u8>,
    /// The joiner's transport public key, bound into the roster fact.
    pub node_pk: String,
    #[serde(with = "super::pake::b64v_pub")]
    pub key_package: Vec<u8>,
    /// Proof of possession (REQ-104): an Ed25519 signature by the DID's
    /// identity key over [`pop_message`]. The inviter verifies it against the
    /// key published in `did_doc`, so a joiner cannot enrol under a DID it
    /// does not control — in particular not the steward's.
    #[serde(with = "super::pake::b64v_pub")]
    pub pop: Vec<u8>,
}

/// The bytes a joiner signs to prove control of its DID (REQ-104): a domain
/// tag, the theory id, and the KeyPackage. Binding the (single-use) KeyPackage
/// ties the proof to this specific enrolment so it cannot be replayed onto a
/// different one.
fn pop_message(theory_id: &str, key_package: &[u8]) -> Vec<u8> {
    let mut m = b"elephant-join-pop-v1\0".to_vec();
    m.extend_from_slice(theory_id.as_bytes());
    m.push(0);
    m.extend_from_slice(key_package);
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use ed25519_dalek::{Signer as _, Verifier as _};

    fn ident(name: &str) -> (tempfile::TempDir, crate::id::Identity) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, Some(name.into())).unwrap();
        (dir, ident)
    }

    /// REQ-104: the proof of possession verifies only for the real DID holder.
    /// Mallory presenting Alice's DID and document — but unable to sign with
    /// Alice's key — is rejected, which is what stops steward impersonation at
    /// admission.
    #[test]
    fn pop_proves_did_control() {
        let (_da, alice) = ident("alice");
        let (_dm, mallory) = ident("mallory");
        let theory = "a".repeat(64);
        let key_package = b"a-key-package".to_vec();
        let msg = pop_message(&theory, &key_package);
        let alice_doc = alice.document.to_bytes().unwrap();

        // The keys published by Alice's document, resolved for Alice's DID.
        let alice_keys = crate::id::verifying_keys_for(&alice_doc, alice.did.as_str());
        assert!(!alice_keys.is_empty(), "alice's doc must publish a key");

        // Alice signs with her identity key → verifies.
        let good = alice.signing_key.sign(&msg);
        assert!(
            alice_keys.iter().any(|k| k.verify(&msg, &good).is_ok()),
            "the real holder's proof must verify"
        );

        // Mallory signs the same message but presents Alice's DID/doc → the
        // signature does not verify under any of Alice's keys.
        let forged = mallory.signing_key.sign(&msg);
        assert!(
            !alice_keys.iter().any(|k| k.verify(&msg, &forged).is_ok()),
            "a proof not signed by the DID's key must be refused"
        );

        // And a document that does not name the claimed DID resolves to no
        // keys at all (fail closed).
        assert!(
            crate::id::verifying_keys_for(&alice_doc, mallory.did.as_str()).is_empty(),
            "a mismatched DID must resolve to no verification keys"
        );
    }
}
