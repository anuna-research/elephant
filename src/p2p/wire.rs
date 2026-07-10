//! Length-prefixed framing and the sync session protocol (SPEC-002 CON-103).
//!
//! Every frame is `u32-be len ‖ bytes`, capped so a hostile peer cannot make
//! us allocate. Framing is transport-agnostic: it runs over the iroh QUIC
//! bi-stream in production and over a duplex pipe in tests, which is what
//! lets the whole join+sync choreography be tested without a network.

use crate::errors::{AppError, AppResult};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// Frame cap. Loro update batches and Welcome messages both fit comfortably;
/// anything larger is a protocol violation, not a big theory.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, bytes: &[u8]) -> AppResult<()> {
    if bytes.len() > MAX_FRAME {
        return Err(AppError::Transport(format!(
            "frame of {} bytes exceeds the {MAX_FRAME}-byte cap",
            bytes.len()
        )));
    }
    w.write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .map_err(tx)?;
    w.write_all(bytes).await.map_err(tx)?;
    w.flush().await.map_err(tx)?;
    Ok(())
}

pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> AppResult<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await.map_err(tx)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        // Refuse before allocating: the length is untrusted input.
        return Err(AppError::Transport(format!(
            "peer announced a {len}-byte frame, over the {MAX_FRAME}-byte cap"
        )));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await.map_err(tx)?;
    Ok(buf)
}

fn tx(e: std::io::Error) -> AppError {
    AppError::Transport(e.to_string())
}

/// ALPN for elephant sync sessions.
pub const ALPN_SYNC: &[u8] = b"elephant/sync/1";
/// ALPN for the SPAKE2 join ceremony.
pub const ALPN_JOIN: &[u8] = b"elephant/join/1";

// ── sync session (CON-103) ─────────────────────────────────────────────

/// Opening frame of a sync session: which theory, and what we already have.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncHello {
    pub v: u16,
    pub theory: String,
    /// Our Loro oplog version vector, opaque bytes.
    #[serde(with = "super::pake::b64v_pub")]
    pub vv: Vec<u8>,
}

pub const SYNC_VERSION: u16 = 1;

/// Exchange version vectors, ship the updates the peer lacks, apply theirs.
/// After a successful round both oplog version vectors are equal (REQ-108).
pub async fn sync_session<S>(
    stream: &mut S,
    theory_id: &str,
    doc: &loro::LoroDoc,
) -> AppResult<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let hello = SyncHello {
        v: SYNC_VERSION,
        theory: theory_id.to_string(),
        vv: doc.oplog_vv().encode(),
    };
    write_frame(stream, &serde_json::to_vec(&hello).map_err(int)?).await?;

    let peer_bytes = read_frame(stream).await?;
    let peer: SyncHello = serde_json::from_slice(&peer_bytes)
        .map_err(|e| AppError::Transport(format!("bad sync hello: {e}")))?;
    if peer.v != SYNC_VERSION {
        return Err(AppError::Transport(format!(
            "peer speaks sync v{}, we speak v{SYNC_VERSION}",
            peer.v
        )));
    }
    if peer.theory != theory_id {
        return Err(AppError::Transport(
            "peer opened a session for a different theory".into(),
        ));
    }

    let peer_vv = loro::VersionVector::decode(&peer.vv)
        .map_err(|e| AppError::Transport(format!("bad version vector: {e}")))?;
    let updates = doc
        .export(loro::ExportMode::updates(&peer_vv))
        .map_err(|e| AppError::Internal(format!("loro export: {e}")))?;
    write_frame(stream, &updates).await?;

    let inbound = read_frame(stream).await?;
    let applied = inbound.len();
    if !inbound.is_empty() {
        // The decoder is the trust boundary for peer bytes; it must never
        // panic the daemon (fuzzed — TEST-113).
        doc.import(&inbound)
            .map_err(|e| AppError::Transport(format!("peer sent an unimportable update: {e}")))?;
    }
    Ok(applied)
}

fn int(e: serde_json::Error) -> AppError {
    AppError::Internal(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn frames_roundtrip() {
        let (mut a, mut b) = duplex(64 * 1024);
        let payload = vec![7u8; 1000];
        let p2 = payload.clone();
        let h = tokio::spawn(async move { write_frame(&mut a, &p2).await.unwrap() });
        let got = read_frame(&mut b).await.unwrap();
        h.await.unwrap();
        assert_eq!(got, payload);
    }

    /// A hostile length header is refused before allocation.
    #[tokio::test]
    async fn oversized_length_refused_before_alloc() {
        let (mut a, mut b) = duplex(64);
        tokio::spawn(async move {
            let _ = a.write_all(&u32::MAX.to_be_bytes()).await;
        });
        let err = read_frame(&mut b).await.unwrap_err();
        assert!(err.to_string().contains("over the"), "{err}");
    }

    /// CON-103: two docs converge and end with equal version vectors.
    #[tokio::test]
    async fn sync_converges_and_equalises_version_vectors() {
        let a = loro::LoroDoc::new();
        a.set_peer_id(1).unwrap();
        a.get_list("corpus").push("alice-entry").unwrap();
        a.commit();

        let b = loro::LoroDoc::new();
        b.set_peer_id(2).unwrap();
        b.get_list("corpus").push("bob-entry").unwrap();
        b.commit();

        let (mut sa, mut sb) = duplex(1024 * 1024);
        let ta = tokio::spawn(async move {
            let doc = a;
            sync_session(&mut sa, "th", &doc).await.unwrap();
            doc
        });
        let tb = tokio::spawn(async move {
            let doc = b;
            sync_session(&mut sb, "th", &doc).await.unwrap();
            doc
        });
        let (a, b) = (ta.await.unwrap(), tb.await.unwrap());

        assert_eq!(a.oplog_vv(), b.oplog_vv(), "version vectors must converge");
        let items = |d: &loro::LoroDoc| -> Vec<String> {
            let mut v: Vec<String> = d
                .get_list("corpus")
                .get_value()
                .into_list()
                .unwrap()
                .iter()
                .map(|x| x.as_string().unwrap().to_string())
                .collect();
            v.sort();
            v
        };
        assert_eq!(items(&a), vec!["alice-entry", "bob-entry"]);
        assert_eq!(items(&a), items(&b));
    }

    /// A session opened for a different theory is refused.
    #[tokio::test]
    async fn theory_mismatch_refused() {
        let a = loro::LoroDoc::new();
        a.set_peer_id(1).unwrap();
        let b = loro::LoroDoc::new();
        b.set_peer_id(2).unwrap();
        let (mut sa, mut sb) = duplex(1024 * 64);
        let ta = tokio::spawn(async move { sync_session(&mut sa, "th-a", &a).await });
        let tb = tokio::spawn(async move { sync_session(&mut sb, "th-b", &b).await });
        let (ra, rb) = (ta.await.unwrap(), tb.await.unwrap());
        assert!(ra.is_err() || rb.is_err(), "mismatched theories must fail");
    }
}
