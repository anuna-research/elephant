//! CLI glue for the p2p commands (SPEC-002): drive the join choreography
//! over a real iroh QUIC stream, and render the roster.

use super::{invite::Invite, join, transport};
use crate::cli::Ctx;
use crate::errors::{AppError, AppResult};
use crate::store::TheoryStore;

/// `elephant theory invite <theory>` (REQ-103): mint a code, bind the
/// rendezvous listener, run the inviter side for the first connection.
pub fn invite(ctx: &Ctx, theory: &str, _ttl: &str) -> AppResult<()> {
    let ident = crate::id::load(&ctx.paths)?;
    let store = TheoryStore::open(&ctx.paths, theory)?;
    let theory_id = store.theory_id.clone();
    drop(store);
    let invite = Invite::generate();

    // The code is printed exactly once, to the TTY (never logged, argv, or
    // JSON — NFR-004 / REQ-103).
    if ctx.json {
        // For scripting, emit only that an invite is pending — never the code.
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": theory_id, "invite_pending": true})
        );
        eprintln!("invite code (share out of band): {invite}");
    } else {
        println!("Share this one-time code with the joiner:\n\n    {invite}\n");
        println!("Waiting for them to join (Ctrl-C to cancel)…");
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| AppError::Internal(e.to_string()))?;
    rt.block_on(async move {
        let listener = transport::rendezvous_listener(&invite).await?;
        let endpoint_hint = transport::addr_hint(&listener);
        let password = invite.password();
        // Accept a single joiner (single-use invite, REQ-110).
        let incoming = listener.accept().await.ok_or_else(|| {
            AppError::Transport("rendezvous closed before a joiner arrived".into())
        })?;
        let conn = incoming
            .await
            .map_err(|e| AppError::Transport(format!("handshake: {e}")))?;
        let (send, recv) = conn
            .accept_bi()
            .await
            .map_err(|e| AppError::Transport(format!("accept stream: {e}")))?;
        let mut stream = tokio::io::join(recv, send);
        let joined = join::inviter_side(
            &mut stream,
            &ctx.paths,
            &ident,
            &theory_id,
            &invite.rendezvous_hint(),
            &password,
            &endpoint_hint,
        )
        .await?;
        listener.close().await;
        Ok::<_, AppError>(joined)
    })
    .map(|did| {
        if !ctx.json {
            println!("admitted {did}");
        }
    })
}

/// `elephant theory join <code>` (REQ-104): dial the rendezvous, run the
/// joiner side, materialise the theory.
pub fn join_theory(ctx: &Ctx, code: Option<&str>, alias: Option<&str>) -> AppResult<()> {
    let ident = crate::id::load(&ctx.paths)?;
    // TTY-only secret entry (REQ-104 / NFR-004): the code never comes from
    // a remote-visible source. When not given as a local argument, prompt.
    let code = match code {
        Some(c) => c.to_string(),
        None => prompt_code()?,
    };
    let invite = super::invite::parse(&code)?;
    let node_pk = transport::node_pk(&ident);

    let rt = tokio::runtime::Runtime::new().map_err(|e| AppError::Internal(e.to_string()))?;
    let theory_id = rt.block_on(async move {
        let conn = transport::dial_rendezvous(&invite).await?;
        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| AppError::Transport(format!("open stream: {e}")))?;
        let mut stream = tokio::io::join(recv, send);
        // The rendezvous only tells us where; the theory id we authenticate
        // is the one carried in the sealed introduction. But SPAKE2 needs a
        // hint bound into its identity — the inviter published under the
        // routing number, so both sides use the theory id the inviter sends.
        // We pass the routing number as the hint; the inviter must use the
        // same. (v0.1: single-theory-per-rendezvous — see ADR note below.)
        let joined = join::joiner_side(
            &mut stream,
            &ctx.paths,
            &ident,
            &invite.rendezvous_hint(),
            &invite.password(),
            &node_pk,
            None,
            alias,
        )
        .await?;
        Ok::<_, AppError>(joined)
    })?;

    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": theory_id, "joined": true})
        );
    } else {
        println!("joined theory {theory_id}");
    }
    Ok(())
}

/// `elephant theory members <theory>` (REQ-105): the closure-derived roster.
pub fn members(ctx: &Ctx, theory: &str) -> AppResult<()> {
    let sub = Ctx {
        paths: ctx.paths.clone(),
        json: ctx.json,
        theory: Some(theory.to_string()),
        at: ctx.at.clone(),
    };
    let view = crate::queries::view(&sub)?;
    // member facts: (member "<did>" "<node-pk>")
    let mut rows = Vec::new();
    for (lit, _) in crate::queries::conclusions_positive(&view) {
        if let Some(rest) = lit.strip_prefix("member ") {
            let parts: Vec<&str> = rest
                .trim_matches(|c| c == '(' || c == ')')
                .split_whitespace()
                .collect();
            if let [did, node] = parts.as_slice() {
                rows.push((
                    did.trim_matches('"').to_string(),
                    node.trim_matches('"').to_string(),
                ));
            }
        }
    }
    rows.sort();
    rows.dedup();
    if ctx.json {
        let items: Vec<_> = rows
            .iter()
            .map(|(d, n)| serde_json::json!({"did": d, "node_pk": n}))
            .collect();
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": view.store.theory_id, "members": items})
        );
    } else if rows.is_empty() {
        println!("(steward only — no joined members yet)");
    } else {
        for (did, _node) in &rows {
            println!("{did}");
        }
    }
    Ok(())
}

/// `elephant theory remove <did>` (SPEC-004 REQ-305): steward removes a
/// member. MLS-removes them (new epoch), rotates the corpus data key,
/// publishes the commit and the rotated keybook on the `mls` lane, and
/// retracts the membership facts so closures reflect it. The removed member
/// is locked out of every entry sealed after this point.
pub fn remove_member(ctx: &Ctx, theory: &str, did: &str) -> AppResult<()> {
    let ident = crate::id::load(&ctx.paths)?;
    let theory_id = remove_member_inner(&ctx.paths, &ident, theory, did)?;
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({"v":1, "theory": theory_id, "removed": did, "rotated": true})
        );
    } else {
        println!("removed {did}; corpus key rotated");
    }
    Ok(())
}

pub(crate) fn remove_member_inner(
    paths: &crate::paths::Paths,
    ident: &crate::id::Identity,
    theory: &str,
    did: &str,
) -> AppResult<String> {
    use crate::core::envelope::{Entry, SpeechAct};
    let mut store = TheoryStore::open(paths, theory)?;
    let theory_id = store.theory_id.clone();

    let provider = crate::e2ee::open_provider(paths, &theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(ident, &theory_id);
    let mut group = crate::e2ee::load_group(&provider, &theory_id)?
        .ok_or_else(|| AppError::Config("no MLS group for this theory".into()))?;

    // Steward check: only the theory creator may remove (v0.1 single
    // committer, SPEC-004 ADR-303). The creator is the genesis signer,
    // located by the genesis hash binding — corpus order is
    // member-controllable via a crafted early HLC.
    let creator = store.steward()?;
    if creator != ident.did.to_string() {
        return Err(AppError::Usage(
            "only the theory's steward (creator) may remove members".into(),
        ));
    }

    // The roster facts to retract: every steward-signed
    // `(given (member "<did>" …))` assert for this member (REQ-105 shape).
    let (entries, _) = store.entries();
    let member_prefix = format!("(member \"{did}\" ");
    let member_sids: Vec<String> = entries
        .iter()
        .filter(|e| e.signer == creator)
        .filter_map(|e| match crate::core::envelope::parse_wire(&e.cbcl) {
            Ok(SpeechAct::Assert { sentence_id, spl })
                if spl.trim().starts_with("(given (member ") && spl.contains(&member_prefix) =>
            {
                Some(sentence_id)
            }
            _ => None,
        })
        .collect();

    // MLS remove → new epoch; rotate the corpus key. Rotation shares the
    // corpus write lock, so no in-flight appender can seal under the old
    // generation once it returns.
    let commit = crate::e2ee::remove_member(&provider, &mut group, &mls_ident, did)?;
    store.rotate_keybook()?;

    // Propagate: the commit moves remaining members to the new epoch, and
    // the rotated keybook — MLS-encrypted to that epoch, so the removed
    // member cannot open it — hands them K_{gen+1}. Both ride the `mls`
    // lane, delivered by every subsequent sync session (REQ-108).
    let keybook = store
        .keybook()
        .ok_or_else(|| AppError::Config("no keybook after rotation".into()))?;
    let kb_msg = crate::e2ee::encrypt_app(&provider, &mut group, &mls_ident, &keybook.to_bytes())?;
    store.push_mls(&[commit, kb_msg])?;

    // Roster update (REQ-305): the roster derives from positive member
    // facts, so a separate `removed` fact alone would leave the DID listed —
    // retract the membership evidence (E1 applies: same signer), then record
    // the removal fact for the audit trail. One batch, HLCs allocated under
    // the clock guard.
    store.bind_identity(ident)?;
    let _clock = store.lock_clock()?;
    let (wall_ms, ts) = crate::cli::now_pair();
    let mut hlc = store.tick(ident, wall_ms);
    let key_id = format!("{}#key-0", ident.did.as_str());
    let mut batch = Vec::with_capacity(member_sids.len() + 1);
    for target in member_sids {
        batch.push(Entry::create(
            &theory_id,
            hlc,
            ident.did.as_str(),
            &key_id,
            &SpeechAct::Retract {
                target,
                reason: format!("member {did} removed by steward"),
            },
            &ts,
            &ident.signing_key,
        ));
        hlc.logical += 1;
    }
    let sid = Entry::sentence_id(&theory_id, ident.did.as_str(), hlc);
    batch.push(Entry::create(
        &theory_id,
        hlc,
        ident.did.as_str(),
        &key_id,
        &SpeechAct::Assert {
            sentence_id: sid,
            spl: format!("(given (removed \"{did}\"))"),
        },
        &ts,
        &ident.signing_key,
    ));
    store.append_batch(&batch)?;
    Ok(theory_id)
}

fn prompt_code() -> AppResult<String> {
    use std::io::Write as _;
    eprint!("invite code: ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| AppError::Usage(format!("could not read code: {e}")))?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::envelope::{Entry, SpeechAct};
    use crate::e2ee::{self, MlsIdentity};
    use crate::paths::Paths;

    struct Member {
        _dir: tempfile::TempDir,
        paths: Paths,
        ident: crate::id::Identity,
    }

    fn member(name: &str) -> Member {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        let ident = crate::id::create(&paths, Some(name.into())).unwrap();
        Member {
            _dir: dir,
            paths,
            ident,
        }
    }

    /// One-directional doc sync (what a sync session does for the receiver).
    fn sync(from: &TheoryStore, to: &TheoryStore) {
        let bytes = from.doc().export(loro::ExportMode::Snapshot).unwrap();
        to.doc().import(&bytes).unwrap();
        to.flush_public().unwrap();
    }

    /// The inviter-side admission choreography, minus the network: MLS add
    /// (commit onto the lane), keybook handover, member fact, store adoption.
    fn admit(steward: &Member, s_store: &TheoryStore, m: &Member, alias: &str) -> TheoryStore {
        let theory_id = s_store.theory_id.clone();
        let s_provider = e2ee::open_provider(&steward.paths, &theory_id).unwrap();
        let s_mls = MlsIdentity::for_theory(&steward.ident, &theory_id);
        let mut s_group = e2ee::load_group(&s_provider, &theory_id).unwrap().unwrap();
        let m_provider = e2ee::open_provider(&m.paths, &theory_id).unwrap();
        let m_mls = MlsIdentity::for_theory(&m.ident, &theory_id);

        let (_bundle, kp_bytes) = e2ee::build_key_package(&m_provider, &m_mls).unwrap();
        let kp =
            e2ee::key_package_from_bytes(&s_provider, &kp_bytes, m.ident.did.as_str()).unwrap();
        let (commit, welcome) = e2ee::add_member(&s_provider, &mut s_group, &s_mls, kp).unwrap();
        s_store.push_mls(&[commit]).unwrap();
        let mut m_group =
            e2ee::join_from_welcome(&m_provider, &welcome, steward.ident.did.as_str()).unwrap();

        let kb_msg = e2ee::encrypt_app(
            &s_provider,
            &mut s_group,
            &s_mls,
            &s_store.keybook().unwrap().to_bytes(),
        )
        .unwrap();
        let keybook = match e2ee::process_inbound(
            &m_provider,
            &mut m_group,
            &kb_msg,
            steward.ident.did.as_str(),
        )
        .unwrap()
        {
            e2ee::Inbound::Application { bytes, .. } => {
                crate::e2ee::keybook::Keybook::from_bytes(&bytes).unwrap()
            }
            other => panic!("expected keybook, got {other:?}"),
        };

        // Membership fact, as the join ceremony records it (REQ-105).
        s_store.bind_identity(&steward.ident).unwrap();
        let clock = s_store.lock_clock().unwrap();
        let (wall_ms, ts) = crate::cli::now_pair();
        let hlc = s_store.tick(&steward.ident, wall_ms);
        let sid = Entry::sentence_id(&theory_id, steward.ident.did.as_str(), hlc);
        let entry = Entry::create(
            &theory_id,
            hlc,
            steward.ident.did.as_str(),
            &format!("{}#key-0", steward.ident.did.as_str()),
            &SpeechAct::Assert {
                sentence_id: sid,
                spl: format!("(given (member \"{}\" \"pk\"))", m.ident.did.as_str()),
            },
            &ts,
            &steward.ident.signing_key,
        );
        s_store.append(&entry).unwrap();
        drop(clock);

        let m_store = TheoryStore::adopt(
            &m.paths,
            &m.ident,
            &theory_id,
            alias,
            keybook,
            steward.ident.did.as_str(),
            &steward.ident.document.to_bytes().unwrap(),
        )
        .unwrap();
        sync(s_store, &m_store);
        m_store
    }

    fn asserted_spls(store: &TheoryStore) -> Vec<String> {
        let (entries, _) = store.entries();
        entries
            .iter()
            .filter_map(|e| crate::core::envelope::parse_wire(&e.cbcl).ok())
            .filter_map(|a| match a {
                SpeechAct::Assert { spl, .. } => Some(spl),
                _ => None,
            })
            .collect()
    }

    /// REQ-305 end to end: removing a member publishes the MLS commit and
    /// the rotated keybook on the `mls` lane, so a REMAINING member reaches
    /// the new epoch, receives K_{gen+1}, and reads post-removal entries —
    /// while the removed member does not. The roster loses the removed DID.
    #[test]
    fn removal_propagates_epoch_keybook_and_roster() {
        let alice = member("alice");
        let a_store = TheoryStore::create(
            &alice.paths,
            &alice.ident,
            "t",
            1_784_000_000_000,
            "2026-07-11T00:00:00Z",
        )
        .unwrap();
        let theory_id = a_store.theory_id.clone();
        let bob = member("bob");
        let carol = member("carol");
        let b_store = admit(&alice, &a_store, &bob, "t");
        let c_store = admit(&alice, &a_store, &carol, "t");

        remove_member_inner(
            &alice.paths,
            &alice.ident,
            &theory_id,
            bob.ident.did.as_str(),
        )
        .unwrap();

        // Steward writes a post-removal entry (sealed under gen 1).
        let a_store = TheoryStore::open(&alice.paths, &theory_id).unwrap();
        a_store.bind_identity(&alice.ident).unwrap();
        let clock = a_store.lock_clock().unwrap();
        let (wall_ms, ts) = crate::cli::now_pair();
        let hlc = a_store.tick(&alice.ident, wall_ms);
        let sid = Entry::sentence_id(&theory_id, alice.ident.did.as_str(), hlc);
        a_store
            .append(&Entry::create(
                &theory_id,
                hlc,
                alice.ident.did.as_str(),
                &format!("{}#key-0", alice.ident.did.as_str()),
                &SpeechAct::Assert {
                    sentence_id: sid,
                    spl: "(given post-removal-secret)".into(),
                },
                &ts,
                &alice.ident.signing_key,
            ))
            .unwrap();
        drop(clock);

        // ── carol (remaining member) syncs and processes the lane.
        sync(&a_store, &c_store);
        assert!(
            e2ee::process_mls_lane(&carol.paths, &c_store).unwrap(),
            "carol must receive the rotated keybook via the lane"
        );
        let c_store = TheoryStore::open(&carol.paths, &theory_id).unwrap();
        let (c_entries, c_malformed) = c_store.entries();
        assert!(
            c_malformed.is_empty(),
            "carol reads post-rotation entries: {c_malformed:?}"
        );
        assert!(
            asserted_spls(&c_store)
                .iter()
                .any(|s| s.contains("post-removal-secret")),
            "carol decrypts the post-removal entry"
        );

        // ── the roster: bob's member fact is retracted, carol's stands.
        let resolve = c_store.key_resolver();
        let closure = crate::core::closure::close(
            &c_entries,
            &theory_id,
            crate::store::GENESIS_THEORY,
            &resolve,
            "",
            1_784_000_600_000,
        )
        .unwrap();
        let members: Vec<String> = closure
            .conclusions
            .iter()
            .filter(|c| c.conclusion_type.is_positive() && !c.literal.negation)
            .map(|c| c.literal.to_spl())
            .filter(|l| l.contains("member "))
            .collect();
        assert!(
            !members.iter().any(|l| l.contains(bob.ident.did.as_str())),
            "removed member must leave the derived roster: {members:?}"
        );
        assert!(
            members.iter().any(|l| l.contains(carol.ident.did.as_str())),
            "remaining member stays on the roster: {members:?}"
        );

        // ── bob (removed) syncs too: the lane must NOT hand him gen 1.
        sync(&a_store, &b_store);
        assert!(
            !e2ee::process_mls_lane(&bob.paths, &b_store).unwrap(),
            "the removed member must not obtain the rotated keybook"
        );
        let b_store = TheoryStore::open(&bob.paths, &theory_id).unwrap();
        let (_, b_malformed) = b_store.entries();
        assert!(
            !b_malformed.is_empty(),
            "post-rotation entries stay sealed away from the removed member"
        );
        assert!(
            !asserted_spls(&b_store)
                .iter()
                .any(|s| s.contains("post-removal-secret")),
            "removed member cannot decrypt post-removal entries"
        );
    }
}
