#![no_main]
//! CON-301: SealedEntry decode + open must fail closed, never panic
//! (TEST-306). Uses a fixed key so decrypt paths are exercised.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(sealed) = elephant::e2ee::seal::from_json(text) {
            let key = [0u8; 32];
            let _ = elephant::e2ee::seal::open(&sealed, "th", &|_| Some(key));
        }
    }
});
