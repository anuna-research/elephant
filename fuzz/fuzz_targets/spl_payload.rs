#![no_main]
//! CON-001: the SPL argument recogniser must never panic on hostile input.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = elephant::core::envelope::validate_assert_payload(text);
    }
});
