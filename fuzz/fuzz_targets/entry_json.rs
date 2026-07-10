#![no_main]
//! CON-002: Entry JSON deserialisation at the corpus trust boundary must
//! never panic (TEST-026).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = elephant::core::envelope::entry_from_json(text);
    }
});
