#![no_main]
//! CON-102: the invite-code recogniser must reject all malformed input
//! without panicking (TEST-113).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = elephant::p2p::invite::parse(text);
    }
});
