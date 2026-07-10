#![no_main]
//! CON-003 stage 2: the cbcl-elephant wire recogniser must never panic on
//! hostile bytes before validation (TEST-026, LangSec full recognition).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = elephant::core::envelope::parse_wire(text);
    }
});
