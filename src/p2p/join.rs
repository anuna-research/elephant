//! The join ceremony choreography (SPEC-002 REQ-104/105/110/111).
//!
//! Written against `AsyncRead + AsyncWrite`, so the whole choreography —
//! SPAKE2, key confirmation, MLS Welcome, sealed introduction, corpus sync —
//! runs identically over an iroh QUIC bi-stream and over an in-memory duplex
//! in tests. Failures after code entry are remotely opaque (`auth-failed`).

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
/// `theory_hint` is bound into the SPAKE2 identity; we use the theory id so
/// a code minted for one theory cannot complete against another.
pub async fn inviter_side<S>(
    stream: &mut S,
    paths: &Paths,
    ident: &Identity,
    theory_id: &str,
    password: &str,
    endpoint_hint: &str,
) -> AppResult<String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 1. SPAKE2
    let (hs, ours) = Handshake::start(password, theory_id, Side::Inviter);
    write_frame(stream, &ours).await?;
    let theirs = read_frame(stream).await?;
    let (keys, confirm) = hs.finish(&theirs)?;

    // 2. Key confirmation BEFORE any payload (REQ-104).
    write_frame(stream, &confirm.ours(&keys)).await?;
    let peer_mac = read_frame(stream).await?;
    confirm.verify_peer(&keys, &peer_mac)?;

    // 3. The joiner proves which DID it is and offers its MLS KeyPackage.
    //    Both are now inside a channel authenticated by the shared secret.
    let hello = read_frame(stream).await?;
    let hello: JoinerHello = serde_json::from_slice(&hello)
        .map_err(|e| AppError::Transport(format!("bad joiner hello: {e}")))?;
    if hello.v != JOINER_HELLO_VERSION {
        return Err(AppError::Signature("auth-failed".into()));
    }

    // 4. MLS Add (steward-only commit) → Welcome for the joiner.
    let store = TheoryStore::open(paths, theory_id)?;
    let provider = crate::e2ee::open_provider(paths, theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, theory_id);
    let mut group = crate::e2ee::load_group(&provider, theory_id)?
        .ok_or_else(|| AppError::Config("no MLS group for this theory".into()))?;
    let kp = crate::e2ee::key_package_from_bytes(&provider, &hello.key_package, &hello.did)?;
    let (_commit, welcome) = crate::e2ee::add_member(&provider, &mut group, &mls_ident, kp)?;

    // 5. Sealed introduction: theory identity + steward DID doc + Welcome.
    let intro = Introduction {
        v: pake::INTRO_VERSION,
        theory_id: theory_id.to_string(),
        alias: store.meta.alias.clone(),
        steward_did: ident.did.to_string(),
        steward_did_doc: ident
            .document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did doc: {e}")))?,
        welcome,
        endpoint: endpoint_hint.to_string(),
    };
    write_frame(stream, &pake::seal_intro(&intro, &keys)?).await?;

    // 6. The keybook, encrypted to the new MLS epoch (REQ-304): only a
    //    current member can read it, and it grants the whole history.
    let keybook = store
        .keybook()
        .ok_or_else(|| AppError::Config("no keybook for this theory".into()))?;
    let kb_msg = crate::e2ee::encrypt_app(&provider, &mut group, &mls_ident, &keybook.to_bytes())?;
    write_frame(stream, &kb_msg).await?;

    // 7. Membership fact into the corpus (REQ-105) — auditable, signed,
    //    and derivable as the roster by every member's closure.
    store.bind_identity(ident)?;
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

    // Persist the joiner's DID document so their signatures verify offline.
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
pub async fn joiner_side<S>(
    stream: &mut S,
    paths: &Paths,
    ident: &Identity,
    theory_hint: &str,
    password: &str,
    node_pk: &str,
    alias_override: Option<&str>,
) -> AppResult<String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 1–2. SPAKE2 + confirmation.
    let (hs, ours) = Handshake::start(password, theory_hint, Side::Joiner);
    let theirs = read_frame(stream).await?;
    write_frame(stream, &ours).await?;
    let (keys, confirm) = hs.finish(&theirs)?;
    let peer_mac = read_frame(stream).await?;
    confirm.verify_peer(&keys, &peer_mac)?;
    write_frame(stream, &confirm.ours(&keys)).await?;

    // 3. Offer our DID + MLS KeyPackage. The theory id is not known yet, so
    //    the KeyPackage is bound to the hint (which IS the theory id — the
    //    inviter published it under the rendezvous record).
    let provider = crate::e2ee::open_provider(paths, theory_hint)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, theory_hint);
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

    // 4. Sealed introduction.
    let intro = pake::open_intro(&read_frame(stream).await?, &keys)?;
    if intro.theory_id != theory_hint {
        return Err(AppError::Signature("auth-failed".into()));
    }

    // 5. Join the MLS group from the Welcome, verifying the steward is the
    //    DID we just authenticated (REQ-302).
    let mut group = crate::e2ee::join_from_welcome(&provider, &intro.welcome, &intro.steward_did)?;

    // 6. Read the keybook from the MLS application message.
    let kb_msg = read_frame(stream).await?;
    let keybook =
        match crate::e2ee::process_inbound(&provider, &mut group, &kb_msg, &intro.steward_did)? {
            crate::e2ee::Inbound::Application(bytes) => {
                crate::e2ee::keybook::Keybook::from_bytes(&bytes)?
            }
            _ => return Err(AppError::Signature("auth-failed".into())),
        };

    // 7. Materialise the theory locally — only now do we touch the store,
    //    so a failure above leaves nothing behind.
    let alias = alias_override.unwrap_or(&intro.alias);
    let store = TheoryStore::adopt(
        paths,
        ident,
        &intro.theory_id,
        alias,
        keybook,
        &intro.steward_did,
        &intro.steward_did_doc,
    )?;

    // 8. Pull the corpus.
    sync_session(stream, &intro.theory_id, store.doc()).await?;
    store.flush_public()?;
    Ok(intro.theory_id)
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
