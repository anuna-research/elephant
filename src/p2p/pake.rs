//! The SPAKE2 join handshake (SPEC-002 REQ-104, ADR-102).
//!
//! Symmetric SPAKE2 over the invite's secret words, then HKDF into two
//! sub-keys: a confirmation MAC key and a sealing key for the introduction
//! payload. Key confirmation happens BEFORE any payload is exchanged, and a
//! failure is remotely indistinguishable (REQ-111).
//!
//! Wire (framed by the caller; each frame is `u32-be len ‖ bytes`):
//! ```text
//!   A → B   spake_msg_a                 (33 bytes)
//!   B → A   spake_msg_b                 (33 bytes)
//!   A → B   mac_a = HMAC(K_conf, "A" ‖ transcript)
//!   B → A   mac_b = HMAC(K_conf, "B" ‖ transcript)
//!   B → A   sealed introduction         (AEAD under K_seal)
//! ```
//! The transcript binds both public messages, so a MITM that relays
//! messages between two honest runs cannot make the MACs agree.

use crate::errors::{AppError, AppResult};
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use hmac::{Hmac, Mac as _};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

type HmacSha256 = Hmac<Sha256>;

const HKDF_SALT: &[u8] = b"elephant/v1";
const INFO_CONF: &[u8] = b"join-confirm";
const INFO_SEAL: &[u8] = b"join-seal";
const AAD_INTRO: &[u8] = b"elephant-intro-v1";

/// Both sides use the same identity string, bound to the theory hint so a
/// code minted for one theory cannot complete against another.
pub fn identity_for(theory_hint: &str) -> Vec<u8> {
    let mut v = b"elephant-join-v1:".to_vec();
    v.extend_from_slice(theory_hint.as_bytes());
    v
}

/// Our half of the exchange, before the peer's message arrives.
pub struct Handshake {
    state: Spake2<Ed25519Group>,
    outbound: Vec<u8>,
    side: Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The inviter (steward).
    Inviter,
    /// The joiner.
    Joiner,
}

impl Side {
    fn tag(&self) -> &'static [u8] {
        match self {
            Side::Inviter => b"I",
            Side::Joiner => b"J",
        }
    }
    fn other(&self) -> Side {
        match self {
            Side::Inviter => Side::Joiner,
            Side::Joiner => Side::Inviter,
        }
    }
}

/// Derived sub-keys; the raw SPAKE2 output is never used directly.
pub struct SessionKeys {
    conf: [u8; 32],
    pub seal: [u8; 32],
}

// Never render key bytes, not even in test failure output.
impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionKeys(<redacted>)")
    }
}

impl Handshake {
    /// Start; returns our outbound SPAKE2 message.
    pub fn start(password: &str, theory_hint: &str, side: Side) -> (Handshake, Vec<u8>) {
        let (state, outbound) = Spake2::<Ed25519Group>::start_symmetric(
            &Password::new(password.as_bytes()),
            &Identity::new(&identity_for(theory_hint)),
        );
        (
            Handshake {
                state,
                outbound: outbound.clone(),
                side,
            },
            outbound,
        )
    }

    /// Finish with the peer's message. A wrong password yields either an
    /// error here or a key mismatch caught at confirmation — both surface
    /// as the same opaque failure to the peer (REQ-111).
    pub fn finish(self, inbound: &[u8]) -> AppResult<(SessionKeys, Confirm)> {
        let side = self.side;
        // Transcript binds both public messages in a canonical order, so
        // both sides compute the same bytes regardless of who spoke first.
        let transcript = transcript(&self.outbound, inbound);
        let raw = self
            .state
            .finish(inbound)
            .map_err(|_| AppError::Signature("auth-failed".into()))?;
        let hk = hkdf::Hkdf::<Sha256>::new(Some(HKDF_SALT), &raw);
        let mut conf = [0u8; 32];
        let mut seal = [0u8; 32];
        hk.expand(INFO_CONF, &mut conf).expect("valid okm");
        hk.expand(INFO_SEAL, &mut seal).expect("valid okm");
        Ok((SessionKeys { conf, seal }, Confirm { transcript, side }))
    }
}

fn transcript(a: &[u8], b: &[u8]) -> Vec<u8> {
    // Canonical (sorted) concatenation with length prefixes: both peers
    // derive identical bytes without agreeing on who is "first".
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut t = Vec::with_capacity(8 + lo.len() + hi.len());
    t.extend_from_slice(&(lo.len() as u32).to_be_bytes());
    t.extend_from_slice(lo);
    t.extend_from_slice(&(hi.len() as u32).to_be_bytes());
    t.extend_from_slice(hi);
    t
}

/// Key confirmation: prove we derived the same key before sending payload.
pub struct Confirm {
    transcript: Vec<u8>,
    side: Side,
}

impl Confirm {
    /// The MAC we send.
    pub fn ours(&self, keys: &SessionKeys) -> Vec<u8> {
        mac(&keys.conf, self.side.tag(), &self.transcript)
    }

    /// Verify the peer's MAC in constant time. On failure the caller MUST
    /// abort with the opaque `auth-failed` and burn the invite (REQ-110).
    pub fn verify_peer(&self, keys: &SessionKeys, peer_mac: &[u8]) -> AppResult<()> {
        let expect = mac(&keys.conf, self.side.other().tag(), &self.transcript);
        let mut h = <HmacSha256 as Mac>::new_from_slice(&keys.conf).expect("hmac key");
        h.update(&expect);
        // Constant-time compare via HmacSha256's verify_slice on a MAC over
        // the expected value — avoids a data-dependent early return.
        let mut check = <HmacSha256 as Mac>::new_from_slice(&keys.conf).expect("hmac key");
        check.update(peer_mac);
        check
            .verify_slice(&h.finalize().into_bytes())
            .map_err(|_| AppError::Signature("auth-failed".into()))
    }
}

use hmac::Mac;

fn mac(key: &[u8; 32], tag: &[u8], transcript: &[u8]) -> Vec<u8> {
    let mut h = <HmacSha256 as Mac>::new_from_slice(key).expect("hmac key");
    h.update(tag);
    h.update(transcript);
    h.finalize().into_bytes().to_vec()
}

/// The inviter's sealed introduction (REQ-104): everything a joiner needs.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Introduction {
    pub v: u16,
    pub theory_id: String,
    pub alias: String,
    /// Steward's DID and serialized DID document (offline verification).
    pub steward_did: String,
    #[serde(with = "b64v_pub")]
    pub steward_did_doc: Vec<u8>,
    /// MLS Welcome for the joiner (SPEC-004).
    #[serde(with = "b64v_pub")]
    pub welcome: Vec<u8>,
    /// Inviter transport address hint (iroh NodeAddr, serialized).
    pub endpoint: String,
}

pub const INTRO_VERSION: u16 = 1;

pub fn seal_intro(intro: &Introduction, keys: &SessionKeys) -> AppResult<Vec<u8>> {
    let pt = serde_json::to_vec(intro).map_err(|e| AppError::Internal(e.to_string()))?;
    let cipher = XChaCha20Poly1305::new(&keys.seal.into());
    let mut nonce = [0u8; 24];
    use rand_core::RngCore as _;
    rand_core::OsRng.fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(
            &nonce.into(),
            Payload {
                msg: &pt,
                aad: AAD_INTRO,
            },
        )
        .map_err(|_| AppError::Internal("seal introduction".into()))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn open_intro(bytes: &[u8], keys: &SessionKeys) -> AppResult<Introduction> {
    if bytes.len() < 24 {
        return Err(AppError::Signature("auth-failed".into()));
    }
    let (nonce, ct) = bytes.split_at(24);
    let nonce: [u8; 24] = nonce.try_into().expect("checked length");
    let cipher = XChaCha20Poly1305::new(&keys.seal.into());
    let pt = cipher
        .decrypt(
            &nonce.into(),
            Payload {
                msg: ct,
                aad: AAD_INTRO,
            },
        )
        .map_err(|_| AppError::Signature("auth-failed".into()))?;
    let intro: Introduction =
        serde_json::from_slice(&pt).map_err(|e| AppError::Parse(format!("introduction: {e}")))?;
    if intro.v != INTRO_VERSION {
        return Err(AppError::Parse(format!(
            "unsupported introduction version {}",
            intro.v
        )));
    }
    Ok(intro)
}

pub(crate) mod b64v_pub {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        B64.decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        pw_a: &str,
        pw_b: &str,
        hint_a: &str,
        hint_b: &str,
    ) -> AppResult<(SessionKeys, SessionKeys)> {
        let (ha, ma) = Handshake::start(pw_a, hint_a, Side::Inviter);
        let (hb, mb) = Handshake::start(pw_b, hint_b, Side::Joiner);
        let (ka, ca) = ha.finish(&mb)?;
        let (kb, cb) = hb.finish(&ma)?;
        // Confirmation both ways, before any payload.
        ca.verify_peer(&ka, &cb.ours(&kb))?;
        cb.verify_peer(&kb, &ca.ours(&ka))?;
        Ok((ka, kb))
    }

    /// HP-O2 happy path: same code, same theory → same keys, intro opens.
    #[test]
    fn matching_password_confirms_and_seals() {
        let (ka, kb) = run("abandon-ability", "abandon-ability", "th", "th").unwrap();
        assert_eq!(ka.seal, kb.seal);

        let intro = Introduction {
            v: INTRO_VERSION,
            theory_id: "th-1".into(),
            alias: "release".into(),
            steward_did: "did:crdt:aa".into(),
            steward_did_doc: b"{}".to_vec(),
            welcome: b"welcome-bytes".to_vec(),
            endpoint: "node-addr".into(),
        };
        let sealed = seal_intro(&intro, &ka).unwrap();
        assert!(
            !String::from_utf8_lossy(&sealed).contains("did:crdt"),
            "sealed introduction must not leak plaintext"
        );
        assert_eq!(open_intro(&sealed, &kb).unwrap(), intro);
    }

    /// REQ-111: a wrong password never yields a shared key, and the failure
    /// is the same opaque `auth-failed` in every branch.
    #[test]
    fn wrong_password_fails_opaquely() {
        let err = run("abandon-ability", "zebra-zone", "th", "th").unwrap_err();
        assert_eq!(err.to_string(), "signature: auth-failed");
    }

    /// The theory hint is bound: a code for one theory cannot complete
    /// against another, even with the right words.
    #[test]
    fn theory_hint_is_bound() {
        let err = run("abandon-ability", "abandon-ability", "th-a", "th-b").unwrap_err();
        assert_eq!(err.to_string(), "signature: auth-failed");
    }

    /// A tampered introduction (or one sealed under a different key) fails
    /// closed with the same message.
    #[test]
    fn tampered_intro_fails_closed() {
        let (ka, _kb) = run("abandon-ability", "abandon-ability", "th", "th").unwrap();
        let (other, _) = run("zebra-zone", "zebra-zone", "th", "th").unwrap();
        let intro = Introduction {
            v: INTRO_VERSION,
            theory_id: "th-1".into(),
            alias: "a".into(),
            steward_did: "did:crdt:aa".into(),
            steward_did_doc: vec![],
            welcome: vec![],
            endpoint: String::new(),
        };
        let sealed = seal_intro(&intro, &ka).unwrap();
        assert!(open_intro(&sealed, &other).is_err(), "wrong key must fail");

        let mut bad = sealed.clone();
        let n = bad.len() - 1;
        bad[n] ^= 0xff;
        assert!(open_intro(&bad, &ka).is_err(), "tampering must fail");
        assert!(open_intro(&sealed[..10], &ka).is_err(), "truncation fails");
    }

    /// A relayed message from a third, honest run does not confirm: the
    /// transcript binds both public messages.
    #[test]
    fn confirmation_binds_the_transcript() {
        let (ha, _ma) = Handshake::start("abandon-ability", "th", Side::Inviter);
        let (hb, mb) = Handshake::start("abandon-ability", "th", Side::Joiner);
        let (hc, mc) = Handshake::start("abandon-ability", "th", Side::Joiner);
        let (ka, ca) = ha.finish(&mb).unwrap();
        let (kb, _cb) = hb.finish(&mc).unwrap(); // b talked to c, not a
        let (kc, cc) = hc.finish(&mb).unwrap();
        let _ = kb;
        // c's MAC was computed over a different transcript → a rejects it.
        let c_mac = cc.ours(&kc);
        assert!(ca.verify_peer(&ka, &c_mac).is_err());
    }
}
