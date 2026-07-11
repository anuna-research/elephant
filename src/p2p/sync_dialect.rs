//! The `cbcl-elephant-sync` dialect (SPEC-002 CON-103): the steady-state
//! sync conversation, modelled as typed CBCL speech acts rather than ad-hoc
//! JSON. Two performatives ride the framed stream:
//!
//! ```text
//!   (lang cbcl-elephant-sync (offer   :theory "<id>" :vv      "<b64 loro vv>"))
//!   (lang cbcl-elephant-sync (deliver :theory "<id>" :updates "<b64 loro>"))
//! ```
//!
//! `offer` announces a theory and the sender's Loro oplog version vector;
//! `deliver` carries the delta bytes the peer lacks. The delta *content* is
//! opaque Loro binary inside a string arg — this dialect types the control
//! envelope, not the CRDT payload; the grow-only guard (`wire.rs`) and the
//! roster gate (`sync.rs`) still police the bytes.
//!
//! Recognition is by hand here, exactly as `envelope::parse_wire` recognises
//! the corpus wire — the dialect registry is an install-time check, not a
//! per-message one. Control messages are unsigned: authority comes from the
//! iroh channel (`Connection::remote_id`) plus the roster binding, not a
//! per-message signature.

use crate::errors::{AppError, AppResult};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use cbcl_core::sexpr::{Atom, SExpr};

pub const SYNC_DIALECT_NAME: &str = "cbcl-elephant-sync";

/// One sync control message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMsg {
    /// "Here is the theory I want and what I already have."
    Offer { theory: String, vv: Vec<u8> },
    /// "Here are the updates you were missing."
    Deliver { theory: String, updates: Vec<u8> },
}

impl SyncMsg {
    pub fn theory(&self) -> &str {
        match self {
            SyncMsg::Offer { theory, .. } | SyncMsg::Deliver { theory, .. } => theory,
        }
    }

    /// Serialise to canonical `cbcl-elephant-sync` text.
    pub fn to_cbcl(&self) -> String {
        let act = match self {
            SyncMsg::Offer { theory, vv } => act("offer", theory, "vv", vv),
            SyncMsg::Deliver { theory, updates } => act("deliver", theory, "updates", updates),
        };
        let lang = SExpr::List(vec![sym("lang"), sym(SYNC_DIALECT_NAME), act]);
        cbcl_core::serializer::serialize(&lang)
    }

    /// Parse and structurally validate one message. Anything that is not a
    /// well-formed `(lang cbcl-elephant-sync (offer|deliver …))` is refused.
    pub fn from_cbcl(text: &str) -> AppResult<SyncMsg> {
        let sexpr = cbcl_parser::parse(text)
            .map_err(|e| AppError::Transport(format!("sync message unparseable: {e:?}")))?;
        let SExpr::List(items) = &sexpr else {
            return Err(bad("root must be a list"));
        };
        // (lang cbcl-elephant-sync <act>)
        match (items.first(), items.get(1)) {
            (Some(SExpr::Atom(Atom::Symbol(head))), Some(SExpr::Atom(Atom::Symbol(name))))
                if head == "lang" && name == SYNC_DIALECT_NAME && items.len() == 3 => {}
            _ => return Err(bad("outer form must be (lang cbcl-elephant-sync …)")),
        }
        let SExpr::List(act) = &items[2] else {
            return Err(bad("performative must be a list"));
        };
        let Some(SExpr::Atom(Atom::Symbol(perf))) = act.first() else {
            return Err(bad("performative head missing"));
        };
        match perf.as_str() {
            "offer" => {
                let theory = kw_str(act, "theory")?;
                let vv = kw_bytes(act, "vv")?;
                Ok(SyncMsg::Offer { theory, vv })
            }
            "deliver" => {
                let theory = kw_str(act, "theory")?;
                let updates = kw_bytes(act, "updates")?;
                Ok(SyncMsg::Deliver { theory, updates })
            }
            other => Err(bad(&format!("unknown sync performative '{other}'"))),
        }
    }
}

fn act(perf: &str, theory: &str, field: &str, bytes: &[u8]) -> SExpr {
    SExpr::List(vec![
        sym(perf),
        kw("theory"),
        string(theory),
        kw(field),
        string(&B64.encode(bytes)),
    ])
}

fn sym(s: &str) -> SExpr {
    SExpr::Atom(Atom::Symbol(s.to_string()))
}
fn kw(s: &str) -> SExpr {
    SExpr::Atom(Atom::Keyword(s.to_string()))
}
fn string(s: &str) -> SExpr {
    SExpr::Atom(Atom::Str(s.to_string()))
}
fn bad(msg: &str) -> AppError {
    AppError::Transport(format!("bad sync message: {msg}"))
}

/// Read the string value following keyword `:name` in a performative list.
fn kw_str(items: &[SExpr], name: &str) -> AppResult<String> {
    let mut i = 1;
    while i + 1 < items.len() {
        if let SExpr::Atom(Atom::Keyword(k)) = &items[i] {
            if k == name {
                return match &items[i + 1] {
                    SExpr::Atom(Atom::Str(v)) => Ok(v.clone()),
                    _ => Err(bad(&format!(":{name} value must be a string"))),
                };
            }
        }
        i += 1;
    }
    Err(bad(&format!("missing :{name}")))
}

fn kw_bytes(items: &[SExpr], name: &str) -> AppResult<Vec<u8>> {
    let s = kw_str(items, name)?;
    B64.decode(s.as_bytes())
        .map_err(|e| bad(&format!(":{name} is not valid base64: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_and_deliver_roundtrip() {
        for msg in [
            SyncMsg::Offer {
                theory: "a".repeat(64),
                vv: vec![1, 2, 3, 0, 255],
            },
            SyncMsg::Deliver {
                theory: "b".repeat(64),
                updates: vec![],
            },
            SyncMsg::Deliver {
                theory: "c".repeat(64),
                updates: (0u8..=255).collect(),
            },
        ] {
            let text = msg.to_cbcl();
            assert!(text.contains(SYNC_DIALECT_NAME));
            assert_eq!(SyncMsg::from_cbcl(&text).unwrap(), msg);
        }
    }

    #[test]
    fn wrong_dialect_is_refused() {
        let text = "(lang cbcl-elephant (offer :theory \"x\" :vv \"AA==\"))";
        assert!(SyncMsg::from_cbcl(text).is_err());
    }

    #[test]
    fn unknown_performative_is_refused() {
        let text = format!("(lang {SYNC_DIALECT_NAME} (gossip :theory \"x\" :vv \"AA==\"))");
        assert!(SyncMsg::from_cbcl(&text).is_err());
    }

    #[test]
    fn bad_base64_is_refused() {
        let text = format!("(lang {SYNC_DIALECT_NAME} (offer :theory \"x\" :vv \"not-b64!!\"))");
        assert!(SyncMsg::from_cbcl(&text).is_err());
    }

    #[test]
    fn missing_field_is_refused() {
        let text = format!("(lang {SYNC_DIALECT_NAME} (offer :theory \"x\"))");
        assert!(SyncMsg::from_cbcl(&text).is_err());
    }
}
