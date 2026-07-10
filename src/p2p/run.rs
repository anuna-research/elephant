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
/// member. MLS-removes them (new epoch), rotates the corpus data key, and
/// asserts the removal so closures reflect it. The removed member is locked
/// out of every entry sealed after this point.
pub fn remove_member(ctx: &Ctx, theory: &str, did: &str) -> AppResult<()> {
    let ident = crate::id::load(&ctx.paths)?;
    let mut store = TheoryStore::open(&ctx.paths, theory)?;
    let theory_id = store.theory_id.clone();

    let provider = crate::e2ee::open_provider(&ctx.paths, &theory_id)?;
    let mls_ident = crate::e2ee::MlsIdentity::for_theory(&ident, &theory_id);
    let mut group = crate::e2ee::load_group(&provider, &theory_id)?
        .ok_or_else(|| AppError::Config("no MLS group for this theory".into()))?;

    // Steward check: only the theory creator may remove (v0.1 single
    // committer, SPEC-004 ADR-303). The creator is the genesis signer.
    let (entries, _) = store.entries();
    let creator = entries
        .first()
        .map(|g| g.signer.clone())
        .ok_or_else(|| AppError::Config("empty corpus: no genesis".into()))?;
    if creator != ident.did.to_string() {
        return Err(AppError::Usage(
            "only the theory's steward (creator) may remove members".into(),
        ));
    }

    // MLS remove → new epoch (the commit would propagate once steady-state
    // sync exists; see SPEC-002 REQ-108).
    let _commit = crate::e2ee::remove_member(&provider, &mut group, &mls_ident, did)?;
    // Rotate the corpus key: the removed member never receives the new keybook.
    store.rotate_keybook()?;

    // Record the removal as a signed corpus fact so the roster closure
    // reflects it (REQ-305).
    store.bind_identity(&ident)?;
    let (wall_ms, ts) = crate::cli::now_pair();
    let hlc = store.tick(&ident, wall_ms);
    let sid = crate::core::envelope::Entry::sentence_id(&theory_id, ident.did.as_str(), hlc);
    let entry = crate::core::envelope::Entry::create(
        &theory_id,
        hlc,
        ident.did.as_str(),
        &format!("{}#key-0", ident.did.as_str()),
        &crate::core::envelope::SpeechAct::Assert {
            sentence_id: sid,
            spl: format!("(given (removed \"{did}\"))"),
        },
        &ts,
        &ident.signing_key,
    );
    store.append(&entry)?;

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
