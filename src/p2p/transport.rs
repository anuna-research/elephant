//! iroh QUIC transport and Mainline-DHT discovery (SPEC-002 ADR-101,
//! REQ-103/104/106/107, CON-104).
//!
//! Two endpoints, two purposes:
//!
//! - **Durable endpoint** — keyed by an HKDF subkey of the agent's identity
//!   seed, so one identity backup restores the transport key too (CON-104).
//!   Its `EndpointId` is published to the DHT and is what peers dial to sync.
//! - **Rendezvous endpoint** — ephemeral, keyed by
//!   `HKDF(invite routing number)`. Both sides derive the same keypair from
//!   the public half of the code, so the joiner can find the inviter without
//!   a tracker. The secret words never touch the DHT; they are only the
//!   SPAKE2 password exercised over the QUIC channel once connected.
//!
//! A resolved DHT record is an unauthenticated *hint*: it confers no
//! authority. Authority comes from SPAKE2 (join) or the roster (sync).

use super::wire::{ALPN_JOIN, ALPN_SYNC};
use crate::errors::{AppError, AppResult};
use crate::id::Identity;
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh_mainline_address_lookup::DhtAddressLookup;

const HKDF_SALT: &[u8] = b"elephant/v1";
const INFO_TRANSPORT: &[u8] = b"transport-key";

fn tx<E: std::fmt::Display>(context: &str) -> impl FnOnce(E) -> AppError + '_ {
    move |e| AppError::Transport(format!("{context}: {e}"))
}

/// The agent's durable transport key (CON-104): derived, not stored twice.
pub fn transport_secret(ident: &Identity) -> SecretKey {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(HKDF_SALT), &ident.seed());
    let mut seed = [0u8; 32];
    hk.expand(INFO_TRANSPORT, &mut seed)
        .expect("32 bytes is a valid okm");
    SecretKey::from_bytes(&seed)
}

/// The rendezvous keypair for an invite — derived from the PUBLIC routing
/// number alone (ADR-102). Deliberately enumerable; carries no secret.
pub fn rendezvous_secret(invite: &super::invite::Invite) -> SecretKey {
    SecretKey::from_bytes(&invite.rendezvous_seed())
}

/// Bind an endpoint that both publishes to and resolves from the Mainline
/// DHT, so peers survive address changes without any operator action.
async fn bind(secret: SecretKey, alpns: Vec<Vec<u8>>) -> AppResult<Endpoint> {
    let dht = DhtAddressLookup::builder()
        .secret_key(secret.clone())
        .build()
        .map_err(tx("dht lookup"))?;
    Endpoint::builder(presets::N0)
        .secret_key(secret)
        .alpns(alpns)
        .address_lookup(dht)
        .bind()
        .await
        .map_err(tx("bind endpoint"))
}

/// The inviter's ephemeral rendezvous listener (REQ-103). Publishing its
/// address under the derived key IS the pkarr record of CON-104: a joiner
/// who knows the routing number resolves it and dials.
pub async fn rendezvous_listener(invite: &super::invite::Invite) -> AppResult<Endpoint> {
    bind(rendezvous_secret(invite), vec![ALPN_JOIN.to_vec()]).await
}

/// The joiner dials the rendezvous by its derived `EndpointId` (REQ-104).
/// Resolution happens through the DHT; the record is only a hint.
pub async fn dial_rendezvous(invite: &super::invite::Invite) -> AppResult<Connection> {
    let target: EndpointId = rendezvous_secret(invite).public();
    // A throwaway identity for the dial: the joiner must not reveal its
    // durable transport key before SPAKE2 has authenticated the peer.
    let ep = Endpoint::builder(presets::N0)
        .secret_key(SecretKey::generate())
        .address_lookup(
            DhtAddressLookup::builder()
                .no_publish()
                .build()
                .map_err(tx("dht lookup"))?,
        )
        .bind()
        .await
        .map_err(tx("bind dialer"))?;
    ep.connect(target, ALPN_JOIN)
        .await
        .map_err(tx("dial rendezvous"))
}

/// The agent's durable sync endpoint (REQ-106): published continuously.
pub async fn sync_endpoint(ident: &Identity) -> AppResult<Endpoint> {
    bind(transport_secret(ident), vec![ALPN_SYNC.to_vec()]).await
}

/// Dial a peer's durable endpoint for a sync session (REQ-107 applies at
/// the accepting side: the roster gate runs before any frame is parsed).
pub async fn dial_sync(ep: &Endpoint, peer: EndpointId) -> AppResult<Connection> {
    ep.connect(peer, ALPN_SYNC).await.map_err(tx("dial peer"))
}

/// Our own transport public key, as it appears in a roster `member` fact.
pub fn node_pk(ident: &Identity) -> String {
    transport_secret(ident).public().to_string()
}

/// Parse a roster-recorded transport key back into a dialable id.
pub fn parse_node_pk(s: &str) -> AppResult<EndpointId> {
    s.parse::<iroh::PublicKey>()
        .map_err(|e| AppError::Parse(format!("bad transport key '{s}': {e}")))
}

/// A serialisable hint for the sealed introduction (never authoritative).
pub fn addr_hint(ep: &Endpoint) -> String {
    serde_json::to_string(&ep.addr()).unwrap_or_default()
}

pub fn parse_addr_hint(s: &str) -> Option<EndpointAddr> {
    serde_json::from_str(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    fn ident(name: &str) -> (tempfile::TempDir, Identity) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let i = crate::id::create(&paths, Some(name.into())).unwrap();
        (dir, i)
    }

    /// CON-104: the transport key is derived, distinct from the identity
    /// signing key, and stable across loads.
    #[test]
    fn transport_key_is_derived_and_stable() {
        let (_d, i) = ident("alice");
        let a = transport_secret(&i);
        let b = transport_secret(&i);
        assert_eq!(a.to_bytes(), b.to_bytes(), "derivation is deterministic");
        assert_ne!(
            a.to_bytes(),
            i.signing_key.to_bytes(),
            "transport key must not be the identity key"
        );
        // And it roundtrips through the roster spelling.
        let pk = node_pk(&i);
        assert_eq!(parse_node_pk(&pk).unwrap(), a.public());
    }

    /// ADR-102: both sides derive the same rendezvous identity from the
    /// public routing number alone — the words never influence it.
    #[test]
    fn rendezvous_identity_is_shared_and_word_independent() {
        let a = super::super::invite::parse("1234-abandon-ability").unwrap();
        let b = super::super::invite::parse("1234-zebra-zone").unwrap();
        let c = super::super::invite::parse("5678-abandon-ability").unwrap();
        assert_eq!(
            rendezvous_secret(&a).public(),
            rendezvous_secret(&b).public(),
            "the joiner finds the inviter from the routing number alone"
        );
        assert_ne!(
            rendezvous_secret(&a).public(),
            rendezvous_secret(&c).public()
        );
    }

    /// The two endpoints a real join uses, bound for real, on one machine:
    /// inviter listens on the rendezvous, joiner dials it by derived id, and
    /// the resulting QUIC bi-stream carries our framing.
    ///
    /// Ignored by default: iroh 1.0 loopback dialing needs endpoint address
    /// discovery to settle, which is slow/flaky in CI. The join choreography
    /// itself is fully covered over an in-memory duplex in
    /// tests/join_ceremony.rs; this test documents the live-transport wiring.
    /// Run with `cargo test -- --ignored` on a networked machine.
    #[tokio::test]
    #[ignore = "needs live iroh endpoint discovery; see tests/join_ceremony.rs"]
    async fn rendezvous_dial_over_quic() {
        use super::super::wire::{read_frame, write_frame};
        use tokio::io::AsyncRead;

        let invite = super::super::invite::Invite::generate();
        // Bind the rendezvous key WITHOUT DHT publish and dial by explicit
        // address: a unit test must not write records to the public Mainline
        // network. The keypair derivation under test is identical either way.
        let listener = Endpoint::builder(presets::Minimal)
            .secret_key(rendezvous_secret(&invite))
            .alpns(vec![ALPN_JOIN.to_vec()])
            .bind()
            .await
            .unwrap();
        // Address the loopback socket explicitly: no relay, no lookup.
        let port = listener
            .bound_sockets()
            .into_iter()
            .find(|s| s.is_ipv4())
            .expect("an ipv4 socket")
            .port();
        let addr = iroh::EndpointAddr::from_parts(
            listener.id(),
            [iroh::TransportAddr::Ip(
                format!("127.0.0.1:{port}").parse().unwrap(),
            )],
        );
        let dialer = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::generate())
            .bind()
            .await
            .unwrap();

        let server = tokio::spawn(async move {
            let incoming = listener.accept().await.expect("incoming");
            let conn = incoming.await.expect("handshake");
            let (mut send, mut recv) = conn.accept_bi().await.expect("accept_bi");
            let got = read_frame(&mut recv).await.unwrap();
            write_frame(&mut send, &got).await.unwrap();
            let _ = send.finish();
            conn.closed().await;
            got
        });

        let conn = dialer.connect(addr, ALPN_JOIN).await.expect("connect");
        let (mut send, mut recv) = conn.open_bi().await.expect("open_bi");
        write_frame(&mut send, b"spake-msg").await.unwrap();
        let echoed = read_frame(&mut recv).await.unwrap();
        assert_eq!(echoed, b"spake-msg");
        let received = server.await.unwrap();
        assert_eq!(received, b"spake-msg");

        // The streams satisfy the bounds our choreography is written against.
        fn assert_async_read<T: AsyncRead>(_: &T) {}
        assert_async_read(&recv);

        conn.close(0u8.into(), b"done");
        dialer.close().await;
    }
}
