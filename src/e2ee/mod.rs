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
    MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyIn, MlsMessageIn,
    MlsMessageOut, ProcessedMessageContent, ProtocolMessage, ProtocolVersion,
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
pub fn add_member(
    provider: &DurableProvider,
    group: &mut MlsGroup,
    ident: &MlsIdentity,
    kp: KeyPackage,
) -> Result<(Vec<u8>, Vec<u8>), MlsError> {
    let (commit, welcome, _) = group
        .add_members(provider, &ident.signer, &[kp])
        .map_err(MlsError::stack("add member"))?;
    group
        .merge_pending_commit(provider)
        .map_err(MlsError::stack("merge add commit"))?;
    provider.persist()?;
    Ok((serialize_out(&commit)?, serialize_out(&welcome)?))
}

/// REQ-305: steward removes a member (new epoch).
pub fn remove_member(
    provider: &DurableProvider,
    group: &mut MlsGroup,
    ident: &MlsIdentity,
    did: &str,
) -> Result<Vec<u8>, MlsError> {
    let target = group
        .members()
        .find(|m| m.credential.serialized_content() == did.as_bytes())
        .ok_or_else(|| MlsError::Rejected(format!("{did} is not a member")))?;
    let (commit, _, _) = group
        .remove_members(provider, &ident.signer, &[target.index])
        .map_err(MlsError::stack("remove member"))?;
    group
        .merge_pending_commit(provider)
        .map_err(MlsError::stack("merge remove commit"))?;
    provider.persist()?;
    serialize_out(&commit)
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
    // The steward must be in the delivered tree under the DID we
    // authenticated via SPAKE2 (REQ-302 / roster binding).
    let steward_present = staged
        .members()
        .any(|m| m.credential.serialized_content() == expected_steward_did.as_bytes());
    if !steward_present {
        provider.rollback_to_disk()?;
        return Err(MlsError::Rejected(format!(
            "welcome tree does not contain the authenticated steward {expected_steward_did}"
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
    Application(Vec<u8>),
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
    let sender_did = processed.credential().serialized_content().to_vec();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => {
            Ok(Inbound::Application(app.into_bytes()))
        }
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            if sender_did != steward_did.as_bytes() {
                return Err(MlsError::Rejected(
                    "only the steward may commit (SPEC-004 ADR-303)".into(),
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
            Inbound::Application(pt) => assert_eq!(pt, b"keybook"),
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
