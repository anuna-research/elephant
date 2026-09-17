//! Portable extensions are ordinary signed, retractable theory metadata.
//! The last active document in corpus order replaces the registry in full.

use crate::errors::{AppError, AppResult};
use spindle_contract::extensions::Extensions;
use spindle_core::{
    FunctionRegistry,
    theory::{MetaValue, Theory},
};

pub const TARGET: &str = "elephant-extensions";
pub const PROPERTY: &str = "document";

pub fn registry(document: &str) -> AppResult<FunctionRegistry> {
    serde_json::from_str::<Extensions>(document)
        .map_err(|e| AppError::Parse(format!("extensions: {e}")))?
        .into_registry()
        .map_err(|e| AppError::Parse(format!("extensions: {e}")))
}

pub fn document(theory: &Theory) -> AppResult<Option<&str>> {
    let Some(meta) = theory.get_meta(TARGET) else {
        return Ok(None);
    };
    match meta.properties.get(PROPERTY) {
        Some(MetaValue::String(s)) => Ok(Some(s)),
        _ => Err(AppError::Parse(
            "extension metadata requires a JSON document string".into(),
        )),
    }
}

pub fn payload(document: &str) -> AppResult<String> {
    registry(document)?;
    // Compact first: SPL unescapes backslash+character literally, so JSON's
    // escapes for formatting newlines would otherwise become bare `n`s.
    let value: serde_json::Value =
        serde_json::from_str(document).map_err(|e| AppError::Parse(e.to_string()))?;
    let compact = value.to_string();
    let quoted = format!("\"{}\"", compact.replace('\\', "\\\\").replace('"', "\\\""));
    Ok(format!("(meta {TARGET} ({PROPERTY} {quoted}))"))
}
