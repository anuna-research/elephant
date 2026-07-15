//! Blocking client for the loopback control API (CLI side of REQ-102).

use super::DiscoveryRecord;
use crate::core::envelope::Entry;
use crate::errors::{AppError, AppResult};
use std::io::BufRead as _;

fn url(rec: &DiscoveryRecord, path: &str) -> String {
    format!("http://{}{}", rec.addr, path)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(500))
        .build()
}

/// Map a daemon request error to an `AppError`, surfacing the daemon's JSON
/// error body on an HTTP status failure. Without this a 4xx collapses to an
/// opaque "status code 400" (ureq's Display drops the response body), so a
/// producer whose assert/promise/define the daemon refused could not see the
/// reason — the CON-004 `{error, detail}` object the daemon actually sent.
fn daemon_err(context: &str, e: ureq::Error) -> AppError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            match daemon_detail(&body) {
                Some(d) => {
                    AppError::Transport(format!("{context}: daemon refused (status {code}): {d}"))
                }
                None => AppError::Transport(format!("{context}: daemon returned status {code}")),
            }
        }
        other => AppError::Transport(format!("{context}: {other}")),
    }
}

/// Pull the human-readable reason out of a daemon error response body: the
/// CON-004 `detail` field, else `error`, else the raw body if it carried no
/// recognisable JSON. None only when the body is empty.
fn daemon_detail(body: &str) -> Option<String> {
    let from_json = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("detail")
                .or_else(|| v.get("error"))
                .and_then(|d| d.as_str())
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty());
    from_json.or_else(|| {
        let trimmed = body.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub fn ping(rec: &DiscoveryRecord) -> bool {
    agent()
        .get(&url(rec, "/v1/ping"))
        .timeout(std::time::Duration::from_millis(500))
        .call()
        .is_ok()
}

pub fn status(rec: &DiscoveryRecord) -> AppResult<serde_json::Value> {
    let resp = agent()
        .get(&url(rec, "/v1/status"))
        .set("authorization", &format!("Bearer {}", rec.token))
        .call()
        .map_err(|e| daemon_err("daemon status", e))?;
    resp.into_json()
        .map_err(|e| AppError::Transport(format!("daemon status body: {e}")))
}

pub fn stop(rec: &DiscoveryRecord) -> AppResult<()> {
    agent()
        .post(&url(rec, "/v1/stop"))
        .set("authorization", &format!("Bearer {}", rec.token))
        .call()
        .map_err(|e| daemon_err("daemon stop", e))?;
    Ok(())
}

/// Route a batch of pre-signed entries through the daemon (single writer).
/// Returns the appended count and the daemon's REQ-406 advisory object,
/// when the append triggered one (SPEC-002 CON-101 advisory extension:
/// `advice` rides a query parameter, enabled by default and sent only as
/// `?advice=false`, NEVER in the body — a pre-advisory daemon's
/// `deny_unknown_fields` body would reject an unknown field, while an
/// unextracted query parameter is ignored, so the request stays valid
/// against every API-v1 daemon in both directions).
pub fn append(
    rec: &DiscoveryRecord,
    theory_id: &str,
    entries: &[Entry],
    advice: bool,
) -> AppResult<(usize, Option<serde_json::Value>)> {
    let body = serde_json::json!({"entries": entries});
    let query = if advice { "" } else { "?advice=false" };
    let resp = agent()
        .post(&url(
            rec,
            &format!("/v1/theories/{theory_id}/entries{query}"),
        ))
        .set("authorization", &format!("Bearer {}", rec.token))
        .send_json(body)
        .map_err(|e| daemon_err("daemon append", e))?;
    let mut v: serde_json::Value = resp
        .into_json()
        .map_err(|e| AppError::Transport(e.to_string()))?;
    let advisory = v.get_mut("advisory").map(serde_json::Value::take);
    Ok((v["appended"].as_u64().unwrap_or(0) as usize, advisory))
}

/// Stream watch events; calls `on_event` per NDJSON line until EOF/error.
pub fn watch(
    rec: &DiscoveryRecord,
    theory_id: &str,
    literals: &[String],
    mut on_event: impl FnMut(serde_json::Value),
) -> AppResult<()> {
    let lits = literals.join(",");
    let resp = ureq::AgentBuilder::new()
        .build()
        .get(&url(
            rec,
            &format!("/v1/theories/{theory_id}/watch?literals={lits}"),
        ))
        .set("authorization", &format!("Bearer {}", rec.token))
        .timeout(std::time::Duration::from_secs(u64::MAX / 4))
        .call()
        .map_err(|e| daemon_err("daemon watch", e))?;
    let reader = std::io::BufReader::new(resp.into_reader());
    for line in reader.lines() {
        let line = line.map_err(|e| AppError::Transport(format!("watch stream: {e}")))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str(&line) {
            on_event(v);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::daemon_detail;

    #[test]
    fn daemon_detail_prefers_detail_then_error_then_raw() {
        // The CON-004 error object the daemon sends on a refused append.
        assert_eq!(
            daemon_detail(r#"{"error":"parse","detail":"bad body: unknown field `advice`"}"#),
            Some("bad body: unknown field `advice`".to_string())
        );
        // detail absent → fall back to the error code.
        assert_eq!(
            daemon_detail(r#"{"error":"auth"}"#),
            Some("auth".to_string())
        );
        // Non-JSON body (e.g. an axum plain-text rejection) → surfaced raw.
        assert_eq!(
            daemon_detail("Failed to deserialize query string"),
            Some("Failed to deserialize query string".to_string())
        );
        // Empty body → nothing to surface.
        assert_eq!(daemon_detail(""), None);
        assert_eq!(daemon_detail("   "), None);
    }
}
