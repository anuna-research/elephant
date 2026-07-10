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
//!   SPAKE2 msgs        (both)
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
    // 1–2. SPAKE2 + key confirmation BEFORE any payload (REQ-104).
    let (hs, ours) = Handshake::start(password, spake_hint, Side::Inviter);
    write_frame(stream, &ours).await?;
    let theirs = read_frame(stream).await?;
    let (keys, confirm) = hs.finish(&theirs)?;
    write_frame(stream, &confirm.ours(&keys)).await?;
    let peer_mac = read_frame(stream).await?;
    confirm.verify_peer(&keys, &peer_mac)?;

    // 3. Sealed introduction — the joiner learns the theory id (and can now
    //    derive its per-theory MLS leaf key).
    let store = TheoryStore::open(paths, theory_id)?;
    let intro = Introduction {
        v: pake::INTRO_VERSION,
        theory_id: theory_id.to_string(),
        alias: store.meta.alias.clone(),
        steward_did: ident.did.to_string(),
        steward_did_doc: ident
            .document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did doc: {e}")))?,
        endpoint: endpoint_hint.to_string(),
    };
    write_frame(stream, &pake::seal_intro(&intro, &keys)?).await?;

    // 4. Joiner hello: DID + MLS KeyPackage (built for this theory).
    let hello = read_frame(stream).await?;
    let hello: JoinerHello = serde_json::from_slice(&hello)
        .map_err(|e| AppError::Transport(format!("bad joiner hello: {e}")))?;
    if hello.v != JOINER_HELLO_VERSION {
        return Err(AppError::Signature("auth-failed".into()));
    }

    // 5. MLS Add (steward-only commit) → Welcome; send it framed.
    let provider = crate::e2ee::open_provider(paths, theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, theory_id);
    let mut group = crate::e2ee::load_group(&provider, theory_id)?
        .ok_or_else(|| AppError::Config("no MLS group for this theory".into()))?;
    let kp = crate::e2ee::key_package_from_bytes(&provider, &hello.key_package, &hello.did)?;
    let (commit, welcome) = crate::e2ee::add_member(&provider, &mut group, &mls_ident, kp)?;
    write_frame(stream, &welcome).await?;
    // Publish the Add commit to the `mls` lane so members other than this
    // joiner advance to the new epoch when they next sync (REQ-108/REQ-306);
    // without it they could not process a later removal commit.
    store.push_mls(&[commit])?;

    // 6. Keybook, encrypted to the new MLS epoch (REQ-304): grants history.
    let keybook = store
        .keybook()
        .ok_or_else(|| AppError::Config("no keybook for this theory".into()))?;
    let kb_msg = crate::e2ee::encrypt_app(&provider, &mut group, &mls_ident, &keybook.to_bytes())?;
    write_frame(stream, &kb_msg).await?;

    // 7. Membership fact into the corpus (REQ-105) — signed, auditable, the
    //    closure-derived roster.
    store.bind_identity(ident)?;
    // HLC allocation and append under one clock guard: a concurrent
    // same-identity writer must not sign the same (wall, logical).
    let _clock = store.lock_clock()?;
    let (wall_ms, ts) = crate::cli::now_pair();
    let hlc = store.tick(ident, wall_ms);
    let sid = Entry::sentence_id(theory_id, ident.did.as_str(), hlc);
    let spl = format!("(given (member \"{}\" \"{}\"))", hello.did, hello.node_pk);
    let entry = Entry::create(
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
    );
    store.append(&entry)?;
    drop(_clock);

    // Persist the joiner's DID document for offline signature verification.
    let member_dir = paths.theory_dir(theory_id).join("members");
    std::fs::create_dir_all(&member_dir)?;
    let did_tail = hello.did.rsplit(':').next().unwrap_or(&hello.did);
    std::fs::write(member_dir.join(format!("{did_tail}.json")), &hello.did_doc)?;

    // 8. Corpus sync: hand over the (sealed) history.
    sync_session(stream, theory_id, store.doc()).await?;
    Ok(hello.did)
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
    // 1–2. SPAKE2 + confirmation.
    let (hs, ours) = Handshake::start(password, spake_hint, Side::Joiner);
    let theirs = read_frame(stream).await?;
    write_frame(stream, &ours).await?;
    let (keys, confirm) = hs.finish(&theirs)?;
    let peer_mac = read_frame(stream).await?;
    confirm.verify_peer(&keys, &peer_mac)?;
    write_frame(stream, &confirm.ours(&keys)).await?;

    // 3. Sealed introduction → learn the theory id and steward DID.
    let intro = pake::open_intro(&read_frame(stream).await?, &keys)?;
    if let Some(want) = expected_theory {
        if want != intro.theory_id {
            return Err(AppError::Signature("auth-failed".into()));
        }
    }
    let theory_id = intro.theory_id.clone();

    // 4. Now that we know the theory, derive the leaf key and offer a
    //    KeyPackage bound to our DID.
    let provider = crate::e2ee::open_provider(paths, &theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, &theory_id);
    let (_bundle, key_package) = crate::e2ee::build_key_package(&provider, &mls_ident)?;
    let hello = JoinerHello {
        v: JOINER_HELLO_VERSION,
        did: ident.did.to_string(),
        did_doc: ident
            .document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did doc: {e}")))?,
        node_pk: node_pk.to_string(),
        key_package,
    };
    write_frame(
        stream,
        &serde_json::to_vec(&hello).map_err(|e| AppError::Internal(e.to_string()))?,
    )
    .await?;

    // 5. MLS Welcome → join the group, verifying the steward is the DID from
    //    the authenticated introduction (REQ-302).
    let welcome = read_frame(stream).await?;
    let mut group = crate::e2ee::join_from_welcome(&provider, &welcome, &intro.steward_did)?;

    // 6. Keybook from the MLS application message.
    let kb_msg = read_frame(stream).await?;
    let keybook =
        match crate::e2ee::process_inbound(&provider, &mut group, &kb_msg, &intro.steward_did)? {
            crate::e2ee::Inbound::Application { sender_did, bytes }
                if sender_did == intro.steward_did =>
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
        &intro.steward_did,
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
    pub did: String,
    #[serde(with = "super::pake::b64v_pub")]
    pub did_doc: Vec<u8>,
    /// The joiner's transport public key, bound into the roster fact.
    pub node_pk: String,
    #[serde(with = "super::pake::b64v_pub")]
    pub key_package: Vec<u8>,
}
