//! Canonical semantic closure fingerprint (#20).
//!
//! Two replicas that reached the same *semantic closure* need a way to check
//! that cheaply, without maintaining bespoke `jq | shasum` pipelines. In the
//! grounded two-machine run, machine A, machine B, and a clean replay all
//! reached the same closure, but raw `status --json` was not directly
//! comparable: conclusion order needed canonical sorting, membership differed
//! per replica/identity, and file-vs-logical newline framing differed by one
//! terminal LF. This module pins that canonicalisation down as a versioned,
//! tested contract.
//!
//! The fingerprint is a function of the *conclusions* alone — the proof tags of
//! every non-membership literal. It deliberately excludes everything that is
//! identity- or deployment-specific rather than semantic: membership,
//! local aliases, theory ids, signers, and timestamps. Two corpora that compute
//! the same conclusions under different identities therefore fingerprint the
//! same; a single changed proof tag changes the digest.
//!
//! Byte framing is load-bearing and fully specified: the canonical sequence is
//! `"{tag} {literal}"` lines, sorted by (literal, tag), de-duplicated, joined
//! with a single `\n`, with **no** terminal newline; the digest is the SHA-256
//! of those bytes, lower-case hex. The `literal` rendering is exactly the one
//! `status --json` emits (a positive atom unparenthesised, a negative literal
//! as `(not …)`), so a fingerprint recomputed from a saved `status --json`
//! artefact is byte-identical to one taken live — which is what makes
//! `closure compare` work on two status files.

use sha2::{Digest, Sha256};

/// Versioned algorithm identifier. Bump the suffix if the canonicalisation
/// (ordering, framing, exclusions, or literal rendering) ever changes, so a
/// digest is never silently compared across incompatible algorithms.
pub const ALGORITHM: &str = "elephant.closure.v1";

/// Hash algorithm backing [`ALGORITHM`].
pub const HASH: &str = "sha256";

/// Metadata classes excluded from the fingerprint by the documented default —
/// everything identity- or deployment-specific rather than semantic.
pub const EXCLUDED: &[&str] = &[
    "membership",
    "local_aliases",
    "theory_id",
    "signers",
    "timestamps",
];

/// A computed closure fingerprint.
pub struct Fingerprint {
    /// Lower-case hex SHA-256 of the canonical byte sequence.
    pub digest: String,
    /// Number of conclusions that fed the digest (post-exclusion, deduped).
    pub included: usize,
    /// The canonical `"{tag} {literal}"` lines, in digest order.
    pub sequence: Vec<String>,
}

impl Fingerprint {
    /// The `sha256:<hex>`-style qualified digest string.
    pub fn qualified(&self) -> String {
        format!("{HASH}:{}", self.digest)
    }
}

/// True for a membership conclusion, which is excluded: it is per-replica
/// (each replica/identity contributes its own `member` fact) and not part of
/// the semantic closure being compared. Recognises **both polarities** — a
/// positive `member …` fact and a negated `(not (member …))` conclusion (which
/// a rule can derive) are equally identity-specific — across the display form
/// (`member "…" "…"`, outer parens stripped) and the raw `to_spl()` form.
fn is_member(literal_display: &str) -> bool {
    let s = literal_display.trim();
    // Unwrap a single `(not …)` so a negated membership literal is caught too;
    // `to_spl()` renders it `(not (member …))`.
    let inner = s
        .strip_prefix("(not ")
        .and_then(|r| r.strip_suffix(')'))
        .map(str::trim)
        .unwrap_or(s);
    is_member_atom(s) || is_member_atom(inner)
}

fn is_member_atom(s: &str) -> bool {
    s == "member"
        || s.starts_with("member ")
        || s.starts_with("(member ")
        || s.starts_with("(member)")
}

/// Compute the fingerprint from `(tag, literal)` pairs, where `literal` is the
/// `status --json` display rendering. Order-independent: the pairs may arrive
/// in any order (journal merge order, JSON order) and the result is identical.
pub fn compute<'a, I>(pairs: I) -> Fingerprint
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    // Sort by (literal, tag) so the ordering is "by literal" as specified, and
    // fully determined even in the (impossible-for-status, possible-for-trace)
    // case of one literal carrying two tags.
    let mut rows: Vec<(String, String)> = pairs
        .into_iter()
        .filter(|(_, lit)| !is_member(lit))
        .map(|(tag, lit)| (lit.to_string(), tag.to_string()))
        .collect();
    rows.sort();
    rows.dedup();
    let sequence: Vec<String> = rows
        .iter()
        .map(|(lit, tag)| format!("{tag} {lit}"))
        .collect();
    // Join with LF and hash WITHOUT a terminal LF — the exact framing the
    // grounded run converged on; a stray terminal newline was one of the three
    // things that made naive hashing disagree.
    let joined = sequence.join("\n");
    let digest = Sha256::digest(joined.as_bytes());
    Fingerprint {
        digest: format!("{digest:x}"),
        included: sequence.len(),
        sequence,
    }
}

/// The fingerprint of a live closure. Uses the same best-tag-per-literal
/// projection as `status`, so `status --json` conclusions and this digest are
/// computed from identical inputs.
pub fn of_closure(closure: &crate::core::closure::Closure) -> Fingerprint {
    let rows = status_rows(closure);
    compute(rows.iter().map(|(t, l)| (t.as_str(), l.as_str())))
}

/// The `(tag, literal-display)` rows `status` presents: best (strongest) tag
/// per display literal, synthetic trigger literals dropped. Kept here so the
/// fingerprint and `status` cannot drift.
pub fn status_rows(closure: &crate::core::closure::Closure) -> Vec<(String, String)> {
    use spindle_core::conclusion::ConclusionType;
    fn rank(t: ConclusionType) -> u8 {
        match t {
            ConclusionType::DefinitelyProvable => 0,
            ConclusionType::DefeasiblyProvable => 1,
            ConclusionType::DefinitelyNotProvable => 2,
            ConclusionType::DefeasiblyNotProvable => 3,
        }
    }
    let mut best: std::collections::BTreeMap<String, ConclusionType> =
        std::collections::BTreeMap::new();
    for c in crate::core::closure::presentable(&closure.conclusions) {
        let key = crate::core::closure::literal_display(&c.literal);
        best.entry(key)
            .and_modify(|t| {
                if rank(c.conclusion_type) < rank(*t) {
                    *t = c.conclusion_type;
                }
            })
            .or_insert(c.conclusion_type);
    }
    best.into_iter()
        .map(|(lit, tag)| (tag.symbol().to_string(), lit))
        .collect()
}

/// Recompute the fingerprint from a saved `status --json` value — the
/// `conclusions` array of `{tag, literal}` objects. Used by `closure compare`
/// so two status artefacts can be checked for semantic convergence without a
/// live daemon.
///
/// Every row is fully validated before hashing: it **rejects** a `conclusions`
/// array whose rows lack a string `tag`/`literal` or carry a tag outside the
/// `+D/+d/-D/-d` set, rather than silently dropping the offending row — a
/// dropped row would let a malformed file falsely compare equal to a shorter
/// one (a false convergence result). The `Err` string names the problem for a
/// caller to surface as a parse error.
pub fn of_status_json(v: &serde_json::Value) -> Result<Fingerprint, String> {
    let arr = v
        .get("conclusions")
        .and_then(|c| c.as_array())
        .ok_or_else(|| "no `conclusions` array — is it `status --json` output?".to_string())?;
    const VALID_TAGS: [&str; 4] = ["+D", "+d", "-D", "-d"];
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(arr.len());
    for (i, row) in arr.iter().enumerate() {
        let tag = row
            .get("tag")
            .and_then(|t| t.as_str())
            .ok_or_else(|| format!("conclusion {i} has no string `tag`"))?;
        let lit = row
            .get("literal")
            .and_then(|l| l.as_str())
            .ok_or_else(|| format!("conclusion {i} has no string `literal`"))?;
        if !VALID_TAGS.contains(&tag) {
            return Err(format!(
                "conclusion {i} has invalid tag {tag:?} (expected one of +D/+d/-D/-d)"
            ));
        }
        pairs.push((tag.to_string(), lit.to_string()));
    }
    Ok(compute(pairs.iter().map(|(t, l)| (t.as_str(), l.as_str()))))
}

/// The fingerprint result as the stable `--json` object (SPEC-001 REQ-024
/// contract), optionally carrying the canonical sequence.
pub fn to_json(fp: &Fingerprint, theory_id: &str, with_sequence: bool) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "v": 1,
        "theory": theory_id,
        "algorithm": ALGORITHM,
        "hash": HASH,
        "digest": fp.digest,
        "included": fp.included,
        "excluded": EXCLUDED,
    });
    if with_sequence {
        obj["sequence"] = serde_json::json!(fp.sequence);
    }
    obj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_independent_and_deduped() {
        let a = compute([("+d", "lab-origin"), ("-D", "(not lab-origin)")]);
        let b = compute([("-D", "(not lab-origin)"), ("+d", "lab-origin")]);
        assert_eq!(a.digest, b.digest, "order must not affect the digest");
        assert_eq!(a.included, 2);
        // Duplicate rows collapse.
        let c = compute([
            ("+d", "lab-origin"),
            ("+d", "lab-origin"),
            ("-D", "(not lab-origin)"),
        ]);
        assert_eq!(c.digest, a.digest);
        assert_eq!(c.included, 2);
    }

    #[test]
    fn members_excluded_both_polarities() {
        let without = compute([("+d", "lab-origin")]);
        let with = compute([
            ("+d", "lab-origin"),
            ("+d", "member \"did:crdt:aa\" \"pk\""),
            ("+d", "(member \"did:crdt:bb\" \"pk\")"),
            // A negated membership conclusion is equally identity-specific.
            ("-D", "(not (member \"did:crdt:cc\" \"pk\"))"),
        ]);
        assert_eq!(
            without.digest, with.digest,
            "membership must not enter the digest, either polarity"
        );
        assert_eq!(with.included, 1);
    }

    #[test]
    fn tag_change_changes_digest() {
        let a = compute([("+d", "lab-origin")]);
        let b = compute([("+D", "lab-origin")]);
        assert_ne!(
            a.digest, b.digest,
            "a changed proof tag must change the digest"
        );
    }

    #[test]
    fn framing_has_no_terminal_newline() {
        // One line: digest is sha256 of the bare line, no trailing LF.
        let fp = compute([("+d", "lab-origin")]);
        let expect = Sha256::digest(b"+d lab-origin");
        assert_eq!(fp.digest, format!("{expect:x}"));
        // Two lines: single LF between, none after.
        let fp2 = compute([("+d", "a"), ("-d", "b")]);
        let expect2 = Sha256::digest(b"+d a\n-d b");
        assert_eq!(fp2.digest, format!("{expect2:x}"));
    }

    #[test]
    fn recompute_from_status_json_matches() {
        let live = compute([("+d", "lab-origin"), ("-D", "(not lab-origin)")]);
        let status = serde_json::json!({
            "conclusions": [
                {"literal": "(not lab-origin)", "tag": "-D"},
                {"literal": "lab-origin", "tag": "+d"},
            ]
        });
        let recomputed = of_status_json(&status).expect("has conclusions");
        assert_eq!(live.digest, recomputed.digest);
    }

    #[test]
    fn malformed_rows_are_rejected_not_dropped() {
        // A row missing its `tag` must be an error — dropping it would let this
        // file falsely compare equal to one holding only the good row.
        let missing_tag = serde_json::json!({
            "conclusions": [{"literal": "x", "tag": "+d"}, {"literal": "y"}]
        });
        assert!(of_status_json(&missing_tag).is_err());
        // Missing `literal`, likewise.
        let missing_lit = serde_json::json!({"conclusions": [{"tag": "+d"}]});
        assert!(of_status_json(&missing_lit).is_err());
        // A tag outside the closed set is rejected.
        let bad_tag = serde_json::json!({
            "conclusions": [{"literal": "x", "tag": "maybe"}]
        });
        assert!(of_status_json(&bad_tag).is_err());
        // No `conclusions` array at all.
        assert!(of_status_json(&serde_json::json!({"nope": 1})).is_err());
    }

    #[test]
    fn empty_closure_is_stable() {
        let a = compute(std::iter::empty());
        let b = compute(std::iter::empty());
        assert_eq!(a.digest, b.digest);
        assert_eq!(a.included, 0);
        assert_eq!(a.digest, format!("{:x}", Sha256::digest(b"")));
    }
}
