//! End-to-end encryption of theory corpora (SPEC-004).
//!
//! One MLS group per theory (group_id = theory id, ADR-301); the steward
//! (creator) is the sole committer (ADR-303). Corpus entries are sealed
//! under random data keys distributed as MLS application messages (the
//! keybook, ADR-302) — so admitted members read the whole history while a
//! removed member is locked out of everything sealed after rotation.

pub mod keybook;
pub mod provider;
pub mod seal;

use crate::errors::{AppError, AppResult};
use crate::id::Identity;
use openmls::prelude::{
    BasicCredential, Ciphersuite, CredentialWithKey, KeyPackage, KeyPackageBundle, KeyPackageIn,
    LeafNodeIndex, MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn,
    MlsMessageIn, MlsMessageOut, ProcessedMessageContent, ProtocolMessage, ProtocolVersion, Sender,
    SenderRatchetConfiguration, StagedWelcome,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::types::SignatureScheme;
use provider::DurableProvider;
use tls_codec::{Deserialize as _, DeserializeBytes as _, Serialize as _};

/// The one ciphersuite (SPEC-004 REQ-301): Ed25519 signatures, so the leaf
/// signer is an Ed25519 key derived from the identity seed (ADR-304).
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

const MAX_PAST_EPOCHS: usize = 2; // bounded tolerance for reordered app msgs
const RESUMPTION_PSKS: usize = 0;
const OUT_OF_ORDER_TOLERANCE: u32 = 5;
const MAX_FORWARD_DISTANCE: u32 = 1000;

/// HKDF label for the per-theory MLS leaf key (CON-304).
const HKDF_SALT: &[u8] = b"elephant/v1";
const INFO_MLS_LEAF: &[u8] = b"mls-leaf";

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("mls stack: {0}")]
    Stack(String),
    #[error("mls storage: {0}")]
    Storage(#[from] std::io::Error),
}

impl From<MlsError> for AppError {
    fn from(e: MlsError) -> AppError {
        match e {
            MlsError::Rejected(m) => AppError::Signature(m),
            MlsError::Stack(m) => AppError::Internal(format!("mls: {m}")),
            MlsError::Storage(e) => AppError::Config(format!("mls state: {e}")),
        }
    }
}

impl MlsError {
    pub(crate) fn stack<E: std::fmt::Debug>(context: &str) -> impl FnOnce(E) -> MlsError + '_ {
        move |e| MlsError::Stack(format!("{context}: {e:?}"))
    }
}

/// Derive the per-theory MLS leaf signing key from the identity seed
/// (CON-304). Distinct per theory; never the identity key itself.
pub fn derive_leaf_key(ident: &Identity, theory_id: &str) -> SignatureKeyPair {
    let mut info = INFO_MLS_LEAF.to_vec();
    info.extend_from_slice(theory_id.as_bytes());
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(HKDF_SALT), &ident.seed());
    let mut seed = [0u8; 32];
    hk.expand(&info, &mut seed)
        .expect("32 bytes is a valid okm");
    let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
    SignatureKeyPair::from_raw(
        SignatureScheme::ED25519,
        sk.to_bytes().to_vec(),
        sk.verifying_key().as_bytes().to_vec(),
    )
}

/// This agent's MLS identity for one theory: DID-bound credential (REQ-302).
pub struct MlsIdentity {
    pub signer: SignatureKeyPair,
    pub credential: CredentialWithKey,
    pub did: String,
}

impl MlsIdentity {
    pub fn for_theory(ident: &Identity, theory_id: &str) -> MlsIdentity {
        let signer = derive_leaf_key(ident, theory_id);
        let credential = CredentialWithKey {
            credential: BasicCredential::new(ident.did.as_str().as_bytes().to_vec()).into(),
            signature_key: signer.public().into(),
        };
        MlsIdentity {
            signer,
            credential,
            did: ident.did.to_string(),
        }
    }

    /// Store the signer so openmls can find it across restarts.
    pub fn store(&self, provider: &DurableProvider) -> Result<(), MlsError> {
        use openmls_traits::OpenMlsProvider as _;
        self.signer
            .store(provider.storage())
            .map_err(MlsError::stack("store signature key"))
    }
}

fn create_config() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .use_ratchet_tree_extension(true)
        .ciphersuite(CIPHERSUITE)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .number_of_resumption_psks(RESUMPTION_PSKS)
        .sender_ratchet_configuration(SenderRatchetConfiguration::new(
            OUT_OF_ORDER_TOLERANCE,
            MAX_FORWARD_DISTANCE,
        ))
        .build()
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .max_past_epochs(MAX_PAST_EPOCHS)
        .number_of_resumption_psks(RESUMPTION_PSKS)
        .sender_ratchet_configuration(SenderRatchetConfiguration::new(
            OUT_OF_ORDER_TOLERANCE,
            MAX_FORWARD_DISTANCE,
        ))
        .build()
}

/// REQ-301: create the theory's MLS group; the creator is the steward.
pub fn create_group(
    provider: &DurableProvider,
    ident: &MlsIdentity,
    theory_id: &str,
) -> Result<MlsGroup, MlsError> {
    ident.store(provider)?;
    let group = MlsGroup::new_with_group_id(
        provider,
        &ident.signer,
        &create_config(),
        openmls::group::GroupId::from_slice(theory_id.as_bytes()),
        ident.credential.clone(),
    )
    .map_err(MlsError::stack("create group"))?;
    provider.persist()?;
    Ok(group)
}

pub fn load_group(
    provider: &DurableProvider,
    theory_id: &str,
) -> Result<Option<MlsGroup>, MlsError> {
    use openmls_traits::OpenMlsProvider as _;
    MlsGroup::load(
        provider.storage(),
        &openmls::group::GroupId::from_slice(theory_id.as_bytes()),
    )
    .map_err(MlsError::stack("load group"))
}

/// A joiner's KeyPackage, published to the inviter over the join channel.
pub fn build_key_package(
    provider: &DurableProvider,
    ident: &MlsIdentity,
) -> Result<(KeyPackageBundle, Vec<u8>), MlsError> {
    ident.store(provider)?;
    let bundle = KeyPackage::builder()
        .build(
            CIPHERSUITE,
            provider,
            &ident.signer,
            ident.credential.clone(),
        )
        .map_err(MlsError::stack("build key package"))?;
    let bytes = bundle
        .key_package()
        .tls_serialize_detached()
        .map_err(MlsError::stack("serialize key package"))?;
    provider.persist()?;
    Ok((bundle, bytes))
}

/// REQ-302: the credential identity must equal the DID authenticated during
/// the join ceremony. Fail closed.
pub fn key_package_from_bytes(
    provider: &DurableProvider,
    bytes: &[u8],
    expected_did: &str,
) -> Result<KeyPackage, MlsError> {
    use openmls_traits::OpenMlsProvider as _;
    // Full recognition before any semantic action: openmls validates the
    // package (signature, lifetime, ciphersuite) before we read its identity.
    let kp_in = KeyPackageIn::tls_deserialize_exact_bytes(bytes)
        .map_err(MlsError::stack("parse key package"))?;
    let kp = kp_in
        .validate(provider.crypto(), ProtocolVersion::Mls10)
        .map_err(MlsError::stack("validate key package"))?;
    let identity = kp.leaf_node().credential().serialized_content();
    if identity != expected_did.as_bytes() {
        return Err(MlsError::Rejected(format!(
            "key package credential identity != authenticated DID {expected_did}"
        )));
    }
    Ok(kp)
}

/// Steward adds a member: returns (commit for the `mls` lane, welcome).
///
/// Crash-safety (SPEC-004 REQ-306): the commit is persisted and recorded in a
/// durable outbox BEFORE our epoch is merged, so if we crash after the merge
/// the outbox still carries the commit and [`recover_outbox`] republishes it —
/// peers can never be left an epoch behind. The caller MUST publish the
/// returned commit to the `mls` lane and then [`clear_outbox`].
pub fn add_member(
    provider: &DurableProvider,
    group: &mut MlsGroup,
    ident: &MlsIdentity,
    kp: KeyPackage,
) -> Result<(Vec<u8>, Vec<u8>), MlsError> {
    let (commit, welcome, _) = group
        .add_members(provider, &ident.signer, &[kp])
        .map_err(MlsError::stack("add member"))?;
    // Persist the pending commit, then record it durably before advancing.
    provider.persist()?;
    let commit_bytes = serialize_out(&commit)?;
    write_outbox(
        provider,
        &Outbox {
            msgs: vec![commit_bytes.clone()],
            remove_pre_gen: None,
        },
    )?;
    group
        .merge_pending_commit(provider)
        .map_err(MlsError::stack("merge add commit"))?;
    provider.persist()?;
    Ok((commit_bytes, serialize_out(&welcome)?))
}

/// REQ-305: steward removes a member (new epoch). `pre_gen` is the keybook
/// generation before the caller rotates it — recorded in the outbox so
/// recovery can complete the rotation exactly once (see [`recover_outbox`]).
/// Same crash-safety contract as [`add_member`].
pub fn remove_member(
    provider: &DurableProvider,
    group: &mut MlsGroup,
    ident: &MlsIdentity,
    did: &str,
    pre_gen: u32,
) -> Result<Vec<u8>, MlsError> {
    let target = group
        .members()
        .find(|m| m.credential.serialized_content() == did.as_bytes())
        .ok_or_else(|| MlsError::Rejected(format!("{did} is not a member")))?;
    let (commit, _, _) = group
        .remove_members(provider, &ident.signer, &[target.index])
        .map_err(MlsError::stack("remove member"))?;
    provider.persist()?;
    let commit_bytes = serialize_out(&commit)?;
    write_outbox(
        provider,
        &Outbox {
            msgs: vec![commit_bytes.clone()],
            remove_pre_gen: Some(pre_gen),
        },
    )?;
    group
        .merge_pending_commit(provider)
        .map_err(MlsError::stack("merge remove commit"))?;
    provider.persist()?;
    serialize_out(&commit)
}

// ── transactional outbox (SPEC-004 REQ-306 crash-safety) ────────────────
//
// A steward that advances the MLS epoch must not do so without a durable
// record of the commit peers need. We write the commit to `mls/outbox.json`
// BEFORE merging our epoch, publish it to the Loro `mls` lane, then clear the
// outbox. A crash between the merge and the publish leaves the outbox behind;
// `recover_outbox` — run before the next steward operation — finishes the
// publication (and, for a removal, completes the keybook rotation exactly
// once via the recorded pre-rotation generation).

#[derive(serde::Serialize, serde::Deserialize)]
struct Outbox {
    /// Lane messages awaiting publication: `[commit]`, or `[commit, keybook]`
    /// once a removal's rotated keybook has been sealed to the new epoch.
    #[serde(with = "outbox_b64")]
    msgs: Vec<Vec<u8>>,
    /// Present iff this is a removal; the keybook generation before rotation,
    /// so recovery rotates at most once (idempotent).
    remove_pre_gen: Option<u32>,
}

fn outbox_path(provider: &DurableProvider) -> std::path::PathBuf {
    provider.path().with_file_name("outbox.json")
}

fn write_outbox(provider: &DurableProvider, ob: &Outbox) -> Result<(), MlsError> {
    let path = outbox_path(provider);
    let bytes = serde_json::to_vec(ob).map_err(std::io::Error::other)?;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    // fsync the temp file, then rename, so the record is durable before we
    // advance the epoch.
    std::fs::File::open(&tmp)?.sync_all()?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

fn read_outbox(provider: &DurableProvider) -> Result<Option<Outbox>, MlsError> {
    match std::fs::read(outbox_path(provider)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(MlsError::Storage(e)),
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| MlsError::Stack(format!("outbox: {e}"))),
    }
}

/// Clear the outbox after the commit (and keybook) are durably on the lane.
pub fn clear_outbox(paths: &crate::paths::Paths, theory_id: &str) -> AppResult<()> {
    let provider = open_provider(paths, theory_id)?;
    let path = outbox_path(&provider);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(MlsError::Storage(e).into()),
    }
}

/// Append the removal's sealed keybook message to the outbox, so recovery can
/// republish the exact bytes rather than re-encrypting (which would ratchet).
/// Called by the steward's removal path after sealing the rotated keybook,
/// before publishing to the lane.
pub fn append_outbox_keybook(
    paths: &crate::paths::Paths,
    theory_id: &str,
    kb_msg: &[u8],
) -> AppResult<()> {
    let provider = open_provider(paths, theory_id)?;
    if let Some(mut ob) = read_outbox(&provider)? {
        ob.msgs.push(kb_msg.to_vec());
        write_outbox(&provider, &ob)?;
    }
    Ok(())
}

/// Finish any interrupted steward epoch change (SPEC-004 REQ-306). Run before
/// a steward starts a new invite/removal, or on daemon start. Idempotent:
/// returns false when there is nothing to recover. Merges a persisted-but-
/// unmerged commit, completes a removal's keybook rotation exactly once, and
/// republishes to the lane unless the commit is already there.
pub fn recover_outbox(
    paths: &crate::paths::Paths,
    store: &crate::store::TheoryStore,
    ident: &Identity,
) -> AppResult<bool> {
    let theory_id = store.theory_id.clone();
    let provider = open_provider(paths, &theory_id)?;
    let Some(mut ob) = read_outbox(&provider)? else {
        return Ok(false);
    };
    let Some(commit) = ob.msgs.first().cloned() else {
        clear_outbox(paths, &theory_id)?;
        return Ok(false);
    };
    let Some(mut group) = load_group(&provider, &theory_id)? else {
        clear_outbox(paths, &theory_id)?;
        return Ok(false);
    };
    let mls_ident = MlsIdentity::for_theory(ident, &theory_id);

    // 1. Advance our epoch if the crash landed before the merge.
    if group.pending_commit().is_some() {
        group
            .merge_pending_commit(&provider)
            .map_err(MlsError::stack("recover: merge pending"))?;
        provider.persist().map_err(AppError::from)?;
    }

    // 2. If the commit already reached the lane, the publish succeeded; just
    //    clear the outbox.
    if store.mls_lane().iter().any(|m| m == &commit) {
        clear_outbox(paths, &theory_id)?;
        return Ok(true);
    }

    // 3. For a removal whose keybook message was not yet sealed, complete the
    //    rotation exactly once (guarded by the recorded pre-rotation
    //    generation) and seal it to the new epoch.
    if let Some(pre_gen) = ob.remove_pre_gen {
        if ob.msgs.len() < 2 {
            let path = keybook::keybook_path(paths, &theory_id);
            let mut kb = keybook::Keybook::load(&path)?
                .ok_or_else(|| AppError::Config("recover: no keybook".into()))?;
            if kb.current == pre_gen {
                kb.rotate();
                kb.save(&path)?;
            }
            let kb_msg = encrypt_app(&provider, &mut group, &mls_ident, &kb.to_bytes())?;
            ob.msgs.push(kb_msg);
        }
    }

    // 4. Publish everything the outbox holds, then clear it.
    store.push_mls(&ob.msgs)?;
    clear_outbox(paths, &theory_id)?;
    Ok(true)
}

mod outbox_b64 {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use serde::{Deserialize, Deserializer, Serializer, ser::SerializeSeq};

    pub fn serialize<S: Serializer>(msgs: &[Vec<u8>], s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(msgs.len()))?;
        for m in msgs {
            seq.serialize_element(&B64.encode(m))?;
        }
        seq.end()
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<u8>>, D::Error> {
        let strs = Vec::<String>::deserialize(d)?;
        strs.into_iter()
            .map(|s| B64.decode(&s).map_err(serde::de::Error::custom))
            .collect()
    }
}

/// Join from a Welcome (the joiner's side of the ceremony).
pub fn join_from_welcome(
    provider: &DurableProvider,
    welcome_bytes: &[u8],
    expected_steward_did: &str,
) -> Result<MlsGroup, MlsError> {
    let msg = MlsMessageIn::tls_deserialize_exact(welcome_bytes)
        .map_err(MlsError::stack("parse welcome"))?;
    let MlsMessageBodyIn::Welcome(welcome) = msg.extract() else {
        return Err(MlsError::Rejected("not a welcome".into()));
    };
    let staged = StagedWelcome::new_from_welcome(provider, &join_config(), welcome, None)
        .map_err(MlsError::stack("stage welcome"))?;
    // The steward must be leaf 0 of the delivered tree (the group creator),
    // carrying the DID we authenticated via SPAKE2 (REQ-302 / roster
    // binding). Pinning leaf 0 — not merely "some leaf" — is what lets later
    // commit authorisation trust the leaf index instead of a spoofable
    // credential string (see `process_inbound`).
    let steward_is_leaf_0 = staged
        .members()
        .find(|m| m.index == LeafNodeIndex::new(0))
        .is_some_and(|m| m.credential.serialized_content() == expected_steward_did.as_bytes());
    if !steward_is_leaf_0 {
        provider.rollback_to_disk()?;
        return Err(MlsError::Rejected(format!(
            "welcome tree's leaf 0 is not the authenticated steward {expected_steward_did}"
        )));
    }
    let group = staged
        .into_group(provider)
        .map_err(MlsError::stack("into group"))?;
    provider.persist()?;
    Ok(group)
}

/// Encrypt an application message (the keybook, CON-303).
pub fn encrypt_app(
    provider: &DurableProvider,
    group: &mut MlsGroup,
    ident: &MlsIdentity,
    plaintext: &[u8],
) -> Result<Vec<u8>, MlsError> {
    let out = group
        .create_message(provider, &ident.signer, plaintext)
        .map_err(MlsError::stack("create message"))?;
    provider.persist()?;
    serialize_out(&out)
}

/// What an inbound `mls` lane element turned out to be.
#[derive(Debug)]
pub enum Inbound {
    /// An application message, with the authenticated sender DID — callers
    /// gate on it (e.g. keybooks are only accepted from the steward).
    Application {
        sender_did: String,
        bytes: Vec<u8>,
    },
    CommitApplied,
    /// A commit whose epoch is ahead of ours; buffer and retry (REQ-306).
    Buffered,
    Ignored,
}

/// Process an inbound MLS message. Only the steward may commit (REQ-306).
pub fn process_inbound(
    provider: &DurableProvider,
    group: &mut MlsGroup,
    bytes: &[u8],
    steward_did: &str,
) -> Result<Inbound, MlsError> {
    let msg =
        MlsMessageIn::tls_deserialize_exact(bytes).map_err(MlsError::stack("parse mls message"))?;
    let protocol: ProtocolMessage = match msg.extract() {
        MlsMessageBodyIn::PrivateMessage(m) => m.into(),
        MlsMessageBodyIn::PublicMessage(m) => m.into(),
        _ => return Ok(Inbound::Ignored),
    };
    if protocol.epoch() > group.epoch() {
        return Ok(Inbound::Buffered);
    }
    if protocol.epoch() < group.epoch() {
        return Ok(Inbound::Ignored); // already applied
    }
    let processed = match group.process_message(provider, protocol) {
        Ok(p) => p,
        Err(e) => return Err(MlsError::Stack(format!("process message: {e:?}"))),
    };
    let sender = processed.sender().clone();
    let sender_did = processed.credential().serialized_content().to_vec();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => Ok(Inbound::Application {
            sender_did: String::from_utf8_lossy(&sender_did).to_string(),
            bytes: app.into_bytes(),
        }),
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            // Authorise the steward by LEAF, not by credential string. The
            // group creator is permanently leaf 0 (openmls never reuses leaf 0
            // while occupied, and a removal blanks a leaf without renumbering
            // the others), so only the holder of leaf 0's current signing key
            // can produce a message openmls attributes to that sender. The
            // credential is an attacker-chosen `BasicCredential` string and is
            // therefore unsafe to authorise on: a second leaf could carry the
            // steward's DID and would pass a string comparison (SPEC-004
            // ADR-303 — steward-only commits). We still bind leaf 0's
            // credential to the authenticated steward DID as a sanity check.
            let steward_leaf = LeafNodeIndex::new(0);
            let is_steward_leaf = sender == Sender::Member(steward_leaf)
                && group
                    .member(steward_leaf)
                    .is_some_and(|c| c.serialized_content() == steward_did.as_bytes());
            if !is_steward_leaf {
                return Err(MlsError::Rejected(
                    "only the steward (group creator, leaf 0) may commit (SPEC-004 ADR-303)".into(),
                ));
            }
            group
                .merge_staged_commit(provider, *staged)
                .map_err(MlsError::stack("merge staged commit"))?;
            provider.persist()?;
            Ok(Inbound::CommitApplied)
        }
        ProcessedMessageContent::ProposalMessage(_)
        | ProcessedMessageContent::ExternalJoinProposalMessage(_) => Err(MlsError::Rejected(
            "proposals are not accepted (steward-only commits)".into(),
        )),
    }
}

pub fn member_dids(group: &MlsGroup) -> Vec<String> {
    group
        .members()
        .map(|m| String::from_utf8_lossy(m.credential.serialized_content()).to_string())
        .collect()
}

fn serialize_out(msg: &MlsMessageOut) -> Result<Vec<u8>, MlsError> {
    msg.tls_serialize_detached()
        .map_err(MlsError::stack("serialize mls message"))
}

/// Open the durable provider for a theory (CON-305).
pub fn open_provider(paths: &crate::paths::Paths, theory_id: &str) -> AppResult<DurableProvider> {
    let dir = paths.theory_dir(theory_id).join("mls");
    std::fs::create_dir_all(&dir)?;
    DurableProvider::open(&dir.join("state.bin")).map_err(Into::into)
}

/// Apply unseen `mls` lane elements (REQ-305/REQ-306): steward commits
/// advance our epoch; steward application messages carry rotated keybooks,
/// which are merged into the on-disk keybook. Returns true when the keybook
/// changed — callers holding an open store should re-open it.
///
/// A cursor under `mls/lane.cursor` makes processing resumable. An
/// epoch-ahead commit stops the scan (its predecessor has not synced in
/// yet — retried next call); an element that fails to process is skipped —
/// it is either one of our own messages echoed back, a replay, or garbage a
/// member injected into the lane, none of which becomes processable later.
pub fn process_mls_lane(
    paths: &crate::paths::Paths,
    store: &crate::store::TheoryStore,
) -> AppResult<bool> {
    let theory_id = store.theory_id.clone();
    let lane = store.mls_lane();
    let cursor_path = paths.theory_dir(&theory_id).join("mls").join("lane.cursor");
    let mut cursor: usize = std::fs::read_to_string(&cursor_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if cursor >= lane.len() {
        return Ok(false);
    }
    let provider = open_provider(paths, &theory_id)?;
    let Some(mut group) = load_group(&provider, &theory_id)? else {
        return Ok(false); // not (or no longer) a group member
    };
    let steward = store.steward()?;
    let mut keybook_changed = false;
    for bytes in &lane[cursor..] {
        match process_inbound(&provider, &mut group, bytes, &steward) {
            Ok(Inbound::Application { sender_did, bytes }) if sender_did == steward => {
                if let Ok(theirs) = keybook::Keybook::from_bytes(&bytes) {
                    let path = keybook::keybook_path(paths, &theory_id);
                    let merged = match keybook::Keybook::load(&path)? {
                        Some(mut mine) => mine.merge(&theirs).is_ok().then_some(mine),
                        None => Some(theirs),
                    };
                    if let Some(kb) = merged {
                        kb.save(&path)?;
                        keybook_changed = true;
                    }
                }
            }
            Ok(Inbound::Buffered) => break,
            Ok(_) | Err(_) => {}
        }
        cursor += 1;
    }
    if let Some(p) = cursor_path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(&cursor_path, cursor.to_string())?;
    Ok(keybook_changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    struct Peer {
        _dir: tempfile::TempDir,
        provider: DurableProvider,
        mls: MlsIdentity,
        did: String,
    }

    fn peer(name: &str, theory: &str) -> Peer {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, Some(name.into())).unwrap();
        let provider = open_provider(&paths, theory).unwrap();
        let mls = MlsIdentity::for_theory(&ident, theory);
        let did = ident.did.to_string();
        Peer {
            _dir: dir,
            provider,
            mls,
            did,
        }
    }

    /// TEST-301/302: group creation, DID-bound credential, add/join.
    #[test]
    fn create_add_join() {
        let theory = "th-e2ee";
        let alice = peer("alice", theory);
        let bob = peer("bob", theory);

        let mut ag = create_group(&alice.provider, &alice.mls, theory).unwrap();
        assert_eq!(member_dids(&ag), vec![alice.did.clone()]);

        let (_bundle, kp_bytes) = build_key_package(&bob.provider, &bob.mls).unwrap();
        // REQ-302: identity binding is checked, fail closed on mismatch.
        assert!(key_package_from_bytes(&alice.provider, &kp_bytes, "did:crdt:wrong").is_err());
        let kp = key_package_from_bytes(&alice.provider, &kp_bytes, &bob.did).unwrap();

        let (_commit, welcome) = add_member(&alice.provider, &mut ag, &alice.mls, kp).unwrap();
        let bg = join_from_welcome(&bob.provider, &welcome, &alice.did).unwrap();

        let mut a_members = member_dids(&ag);
        let mut b_members = member_dids(&bg);
        a_members.sort();
        b_members.sort();
        assert_eq!(a_members, b_members);
        assert_eq!(a_members.len(), 2);

        // A welcome whose tree lacks the authenticated steward is rejected.
        let carol = peer("carol", theory);
        assert!(join_from_welcome(&carol.provider, &welcome, "did:crdt:not-the-steward").is_err());
    }

    /// TEST-307: only the steward may commit; app messages flow both ways.
    #[test]
    fn steward_only_commits_and_app_messages() {
        let theory = "th-commit";
        let alice = peer("alice", theory);
        let bob = peer("bob", theory);
        let mut ag = create_group(&alice.provider, &alice.mls, theory).unwrap();
        let (_b, kp_bytes) = build_key_package(&bob.provider, &bob.mls).unwrap();
        let kp = key_package_from_bytes(&alice.provider, &kp_bytes, &bob.did).unwrap();
        let (_commit, welcome) = add_member(&alice.provider, &mut ag, &alice.mls, kp).unwrap();
        let mut bg = join_from_welcome(&bob.provider, &welcome, &alice.did).unwrap();

        // Steward → member application message.
        let ct = encrypt_app(&alice.provider, &mut ag, &alice.mls, b"keybook").unwrap();
        match process_inbound(&bob.provider, &mut bg, &ct, &alice.did).unwrap() {
            Inbound::Application { sender_did, bytes } => {
                assert_eq!(bytes, b"keybook");
                assert_eq!(sender_did, alice.did, "sender must be authenticated");
            }
            _ => panic!("expected application message"),
        }

        // A non-steward commit is rejected by the steward's own processor.
        let carol = peer("carol", theory);
        let (_b2, kp2) = build_key_package(&carol.provider, &carol.mls).unwrap();
        let kp2 = key_package_from_bytes(&bob.provider, &kp2, &carol.did).unwrap();
        let (bob_commit, _w) = add_member(&bob.provider, &mut bg, &bob.mls, kp2).unwrap();
        let err = process_inbound(&alice.provider, &mut ag, &bob_commit, &alice.did);
        assert!(
            matches!(err, Err(MlsError::Rejected(_))),
            "non-steward commit must be rejected: {err:?}"
        );
    }

    /// SPEC-004 REQ-302/ADR-303 regression: a member whose credential string
    /// is forged to equal the steward's DID STILL cannot commit. Authorisation
    /// is by leaf index (the steward is permanently leaf 0), not by the
    /// attacker-chosen credential bytes — so the impersonation is refused even
    /// though `sender_did == steward_did` would have accepted it.
    #[test]
    fn forged_steward_credential_cannot_commit() {
        let theory = "th-impersonate";
        let alice = peer("alice", theory); // steward, leaf 0
        let bob = peer("bob", theory);
        let carol = peer("carol", theory);

        let mut ag = create_group(&alice.provider, &alice.mls, theory).unwrap();

        // Bob enrols carrying a FORGED credential whose identity bytes are
        // Alice's DID, but signed by a distinct leaf key he controls.
        let rogue_signer = SignatureKeyPair::new(SignatureScheme::ED25519).unwrap();
        let rogue = MlsIdentity {
            credential: CredentialWithKey {
                credential: BasicCredential::new(alice.did.as_bytes().to_vec()).into(),
                signature_key: rogue_signer.public().into(),
            },
            signer: rogue_signer,
            did: alice.did.clone(),
        };
        let (_b, kp_bytes) = build_key_package(&bob.provider, &rogue).unwrap();
        // The identity check passes — the credential DOES equal the claimed
        // DID; that check alone is not enough, which is the whole point.
        let kp = key_package_from_bytes(&alice.provider, &kp_bytes, &alice.did).unwrap();
        let (_c, welcome) = add_member(&alice.provider, &mut ag, &alice.mls, kp).unwrap();
        let mut bg = join_from_welcome(&bob.provider, &welcome, &alice.did).unwrap();

        // Bob (leaf 1, forged "alice" credential) issues a commit.
        let (_c2, ckp) = build_key_package(&carol.provider, &carol.mls).unwrap();
        let ckp = key_package_from_bytes(&bob.provider, &ckp, &carol.did).unwrap();
        let (bob_commit, _w) = add_member(&bob.provider, &mut bg, &rogue, ckp).unwrap();

        // Alice's processor rejects it: the sender is leaf 1, not the steward.
        let err = process_inbound(&alice.provider, &mut ag, &bob_commit, &alice.did);
        assert!(
            matches!(err, Err(MlsError::Rejected(_))),
            "a forged-credential non-leaf-0 commit must be rejected: {err:?}"
        );
    }

    /// TEST-308: MLS state survives a restart (fresh provider, same path).
    #[test]
    fn state_survives_restart() {
        let theory = "th-restart";
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, Some("alice".into())).unwrap();
        {
            let provider = open_provider(&paths, theory).unwrap();
            let mls = MlsIdentity::for_theory(&ident, theory);
            create_group(&provider, &mls, theory).unwrap();
        }
        let provider = open_provider(&paths, theory).unwrap();
        let group = load_group(&provider, theory).unwrap();
        assert!(group.is_some(), "group must reload after restart");
    }

    /// ADR-304: leaf keys are per-theory and never the identity key.
    #[test]
    fn leaf_keys_are_derived_and_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, None).unwrap();
        let a = derive_leaf_key(&ident, "theory-a");
        let b = derive_leaf_key(&ident, "theory-b");
        assert_ne!(a.public(), b.public());
        assert_ne!(a.public(), ident.signing_key.verifying_key().as_bytes());
    }
}
