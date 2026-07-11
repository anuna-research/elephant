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

/// Exchange version vectors, ship the updates the peer lacks, apply theirs.
/// After a successful round both oplog version vectors are equal (REQ-108).
///
/// The wire is the `cbcl-elephant-sync` dialect (see [`super::sync_dialect`]):
/// each side sends an `offer` (its version vector), then a `deliver` (the
/// updates the peer was missing). Typed, recognised speech acts rather than
/// ad-hoc JSON.
pub async fn sync_session<S>(
    stream: &mut S,
    theory_id: &str,
    doc: &loro::LoroDoc,
) -> AppResult<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use super::sync_dialect::SyncMsg;

    // 1. Offer our version vector.
    let offer = SyncMsg::Offer {
        theory: theory_id.to_string(),
        vv: doc.oplog_vv().encode(),
    };
    write_frame(stream, offer.to_cbcl().as_bytes()).await?;

    // 2. Read the peer's offer; it must be for the same theory.
    let peer_vv = match read_sync_msg(stream).await? {
        SyncMsg::Offer { theory, vv } if theory == theory_id => loro::VersionVector::decode(&vv)
            .map_err(|e| AppError::Transport(format!("bad version vector: {e}")))?,
        SyncMsg::Offer { .. } => {
            return Err(AppError::Transport(
                "peer opened a session for a different theory".into(),
            ));
        }
        SyncMsg::Deliver { .. } => {
            return Err(AppError::Transport(
                "expected an offer to open the sync session".into(),
            ));
        }
    };

    // 3. Deliver the updates the peer lacks.
    let updates = doc
        .export(loro::ExportMode::updates(&peer_vv))
        .map_err(|e| AppError::Internal(format!("loro export: {e}")))?;
    let deliver = SyncMsg::Deliver {
        theory: theory_id.to_string(),
        updates,
    };
    write_frame(stream, deliver.to_cbcl().as_bytes()).await?;

    // 4. Read the peer's delivery and grow-only import it.
    let inbound = match read_sync_msg(stream).await? {
        SyncMsg::Deliver { theory, updates } if theory == theory_id => updates,
        SyncMsg::Deliver { .. } => {
            return Err(AppError::Transport(
                "peer delivered for a different theory".into(),
            ));
        }
        SyncMsg::Offer { .. } => {
            return Err(AppError::Transport("expected a delivery".into()));
        }
    };
    let applied = inbound.len();
    if !inbound.is_empty() {
        // The decoder is the trust boundary for peer bytes; it must never
        // panic the daemon (fuzzed — TEST-113), and it must not let a peer
        // DELETE from the append-only log (grow-only enforcement).
        import_grow_only(doc, &inbound)?;
    }
    Ok(applied)
}

/// Read one framed `cbcl-elephant-sync` message.
async fn read_sync_msg<S>(stream: &mut S) -> AppResult<super::sync_dialect::SyncMsg>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let frame = read_frame(stream).await?;
    let text = std::str::from_utf8(&frame)
        .map_err(|_| AppError::Transport("sync message is not valid utf-8".into()))?;
    super::sync_dialect::SyncMsg::from_cbcl(text)
}

async fn send_sync_msg<S>(stream: &mut S, msg: &super::sync_dialect::SyncMsg) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    write_frame(stream, msg.to_cbcl().as_bytes()).await
}

/// Read the opening `offer` of a session (the responder/accept side), so the
/// caller can learn which theory the peer wants and run the roster gate
/// (REQ-107) BEFORE opening any store. Returns `(theory, encoded peer vv)`.
pub async fn read_opening_offer<S>(stream: &mut S) -> AppResult<(String, Vec<u8>)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use super::sync_dialect::SyncMsg;
    match read_sync_msg(stream).await? {
        SyncMsg::Offer { theory, vv } => Ok((theory, vv)),
        SyncMsg::Deliver { .. } => Err(AppError::Transport(
            "expected an offer to open the sync session".into(),
        )),
    }
}

/// Complete a sync session as the RESPONDER, after [`read_opening_offer`] (the
/// caller has run the roster gate and opened `doc` for `theory_id`).
/// `peer_vv_encoded` is the version vector from the opening offer. Mirrors
/// [`sync_session`] but with the opening offer already consumed.
pub async fn respond_sync_session<S>(
    stream: &mut S,
    theory_id: &str,
    doc: &loro::LoroDoc,
    peer_vv_encoded: &[u8],
) -> AppResult<usize>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use super::sync_dialect::SyncMsg;
    let peer_vv = loro::VersionVector::decode(peer_vv_encoded)
        .map_err(|e| AppError::Transport(format!("bad version vector: {e}")))?;

    // Our offer, then the updates the peer lacked (computed before importing).
    send_sync_msg(
        stream,
        &SyncMsg::Offer {
            theory: theory_id.to_string(),
            vv: doc.oplog_vv().encode(),
        },
    )
    .await?;
    let updates = doc
        .export(loro::ExportMode::updates(&peer_vv))
        .map_err(|e| AppError::Internal(format!("loro export: {e}")))?;

    // Receive the peer's delivery and grow-only import it, then send ours.
    let inbound = match read_sync_msg(stream).await? {
        SyncMsg::Deliver { theory, updates } if theory == theory_id => updates,
        SyncMsg::Deliver { .. } => {
            return Err(AppError::Transport(
                "peer delivered for a different theory".into(),
            ));
        }
        SyncMsg::Offer { .. } => {
            return Err(AppError::Transport("expected a delivery".into()));
        }
    };
    let applied = inbound.len();
    if !inbound.is_empty() {
        import_grow_only(doc, &inbound)?;
    }
    send_sync_msg(
        stream,
        &SyncMsg::Deliver {
            theory: theory_id.to_string(),
            updates,
        },
    )
    .await?;
    Ok(applied)
}

/// Apply a peer's Loro update only if it is GROW-ONLY. The corpus is an
/// insert-only log (SPEC-001 ADR-002) and the `mls` lane likewise only ever
/// grows. But Loro lists support deletion, so a hostile peer can craft a CRDT
/// update that removes existing elements — deleting genesis would orphan the
/// theory (`steward()` would then fail). We stage the update on an independent
/// fork and refuse it if any element currently present would disappear, before
/// it can touch the live doc.
fn import_grow_only(doc: &loro::LoroDoc, update: &[u8]) -> AppResult<()> {
    use crate::store::{CORPUS_CONTAINER, MLS_CONTAINER};
    let staged = doc.fork();
    staged
        .import(update)
        .map_err(|e| AppError::Transport(format!("peer sent an unimportable update: {e}")))?;
    for container in [CORPUS_CONTAINER, MLS_CONTAINER] {
        let before = list_multiset(doc, container);
        let after = list_multiset(&staged, container);
        for (elem, n) in &before {
            if after.get(elem).copied().unwrap_or(0) < *n {
                return Err(AppError::Transport(format!(
                    "peer update removes '{container}' entries; the log is append-only"
                )));
            }
        }
    }
    doc.import(update)
        .map_err(|e| AppError::Transport(format!("peer sent an unimportable update: {e}")))?;
    Ok(())
}

/// The string elements of a Loro list container, counted (a multiset): the
/// corpus and `mls` lanes are lists of opaque strings, and we compare counts
/// so a delete of a (rare) duplicated element is still caught.
fn list_multiset(doc: &loro::LoroDoc, container: &str) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    let values = doc
        .get_list(container)
        .get_value()
        .into_list()
        .unwrap_or_default();
    for v in values.iter() {
        if let Some(s) = v.as_string() {
            *counts.entry(s.to_string()).or_insert(0) += 1;
        }
    }
    counts
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

    /// SPEC-001 ADR-002 regression: a peer update that DELETES a corpus
    /// element (here genesis) is refused, and the live doc keeps every entry.
    #[test]
    fn grow_only_rejects_deletion() {
        let doc = loro::LoroDoc::new();
        doc.set_peer_id(1).unwrap();
        doc.get_list(crate::store::CORPUS_CONTAINER)
            .push("genesis")
            .unwrap();
        doc.get_list(crate::store::CORPUS_CONTAINER)
            .push("entry-2")
            .unwrap();
        doc.commit();

        // A hostile peer forks the state and deletes genesis, then ships the
        // resulting update.
        let attacker = doc.fork();
        attacker
            .get_list(crate::store::CORPUS_CONTAINER)
            .delete(0, 1)
            .unwrap();
        attacker.commit();
        let malicious = attacker
            .export(loro::ExportMode::updates(&doc.oplog_vv()))
            .unwrap();

        let err = import_grow_only(&doc, &malicious).unwrap_err();
        assert!(
            err.to_string().contains("append-only"),
            "a deletion must be refused: {err}"
        );
        // The live doc is untouched: both entries remain.
        assert_eq!(list_multiset(&doc, crate::store::CORPUS_CONTAINER).len(), 2);
    }

    /// A grow-only update (pure insert) is accepted.
    #[test]
    fn grow_only_accepts_insert() {
        let doc = loro::LoroDoc::new();
        doc.set_peer_id(1).unwrap();
        doc.get_list(crate::store::CORPUS_CONTAINER)
            .push("genesis")
            .unwrap();
        doc.commit();

        let peer = doc.fork();
        peer.set_peer_id(2).unwrap();
        peer.get_list(crate::store::CORPUS_CONTAINER)
            .push("peer-entry")
            .unwrap();
        peer.commit();
        let update = peer
            .export(loro::ExportMode::updates(&doc.oplog_vv()))
            .unwrap();

        import_grow_only(&doc, &update).unwrap();
        assert_eq!(list_multiset(&doc, crate::store::CORPUS_CONTAINER).len(), 2);
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
