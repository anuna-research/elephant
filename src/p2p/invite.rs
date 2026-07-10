//! Invite codes (SPEC-002 CON-102) — the single recogniser for the
//! `NNNN-word-word` grammar, and the routing/secret split.
//!
//! ```abnf
//! code   = number "-" word "-" word
//! number = 4*5DIGIT   ; ROUTING ONLY: derives the pkarr rendezvous keypair.
//!                     ; Public and enumerable; carries no secret.
//! word   = 3*8ALPHA   ; SECRET: "word-word" is the SPAKE2 password.
//!                     ; BIP39 English list (2048 words) → 22 bits (NFR-104).
//! ```
//!
//! The number never carries entropy that matters and the words never reach
//! the DHT. That split is inherited from [[SPEC-047]] ADR-473, and the
//! 22-bit floor is defensible *only* with single-use invites (REQ-110) —
//! SPAKE2 gives an attacker exactly one online guess per run.

use crate::errors::{AppError, AppResult};

/// Vendored from ../hark/src/pairing/bip39-english.txt (BIP-39 English).
const WORDLIST: &str = include_str!("../../vendor/bip39-english.txt");

const MIN_DIGITS: usize = 4;
const MAX_DIGITS: usize = 5;
const MIN_WORD: usize = 3;
const MAX_WORD: usize = 8;

/// HKDF info label for the rendezvous keypair (CON-304 domain separation).
const INFO_RENDEZVOUS: &[u8] = b"elephant/rdv/v1";
const HKDF_SALT: &[u8] = b"elephant/v1";

pub fn words() -> Vec<&'static str> {
    WORDLIST
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && w.len() >= MIN_WORD && w.len() <= MAX_WORD && w.is_ascii())
        .collect()
}

/// A parsed invite code. `Display` renders the canonical spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// Public routing component (as typed — leading zeros are significant).
    pub number: String,
    /// Secret words; `word-word` is the SPAKE2 password.
    pub w1: String,
    pub w2: String,
}

impl Invite {
    /// The SPAKE2 password: exactly the two words, hyphenated. Never logged.
    pub fn password(&self) -> String {
        format!("{}-{}", self.w1, self.w2)
    }

    /// Mint a fresh code from the OS RNG (REQ-103).
    pub fn generate() -> Invite {
        use rand_core::RngCore as _;
        let list = words();
        let mut rng = rand_core::OsRng;
        // 4-digit routing number, uniform in [1000, 9999] (no leading zero,
        // so the canonical spelling roundtrips). Rejection sampling keeps it
        // unbiased.
        let number = loop {
            let n = rng.next_u32() % 10_000;
            if n >= 1000 {
                break n.to_string();
            }
        };
        let pick = |rng: &mut rand_core::OsRng| -> String {
            let mut idx;
            // Rejection-sample to avoid modulo bias over the list length.
            let bound = u32::MAX - (u32::MAX % list.len() as u32);
            loop {
                idx = rng.next_u32();
                if idx < bound {
                    break;
                }
            }
            list[(idx as usize) % list.len()].to_string()
        };
        let w1 = pick(&mut rng);
        let w2 = pick(&mut rng);
        Invite { number, w1, w2 }
    }

    /// The SPAKE2 identity hint: the public routing number. Shared by both
    /// sides via the code; the theory id is NOT used (the joiner does not
    /// know it until the sealed introduction).
    pub fn rendezvous_hint(&self) -> String {
        format!("rdv:{}", self.number)
    }

    /// Rendezvous keypair seed: `HKDF(number)` — routing only (ADR-102).
    /// The words are NOT an input: they must never reach the DHT.
    pub fn rendezvous_seed(&self) -> [u8; 32] {
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(HKDF_SALT), self.number.as_bytes());
        let mut out = [0u8; 32];
        hk.expand(INFO_RENDEZVOUS, &mut out)
            .expect("32 bytes is a valid okm");
        out
    }
}

impl std::fmt::Display for Invite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.number, self.w1, self.w2)
    }
}

/// The single recogniser for the invite grammar (CON-102, LangSec:
/// one parser per language, conservative in what it accepts).
pub fn parse(input: &str) -> AppResult<Invite> {
    let s = input.trim();
    let mut parts = s.split('-');
    let (Some(number), Some(w1), Some(w2), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(AppError::Parse(
            "invite code must be exactly number-word-word".into(),
        ));
    };
    if number.len() < MIN_DIGITS
        || number.len() > MAX_DIGITS
        || !number.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(AppError::Parse(format!(
            "invite routing number must be {MIN_DIGITS}-{MAX_DIGITS} digits"
        )));
    }
    for w in [w1, w2] {
        if w.len() < MIN_WORD || w.len() > MAX_WORD || !w.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(AppError::Parse(format!(
                "invite words must be {MIN_WORD}-{MAX_WORD} lowercase ASCII letters"
            )));
        }
    }
    Ok(Invite {
        number: number.to_string(),
        w1: w1.to_string(),
        w2: w2.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_codes_roundtrip() {
        for _ in 0..64 {
            let i = Invite::generate();
            let text = i.to_string();
            assert_eq!(parse(&text).unwrap(), i, "roundtrip failed for {text}");
            assert_eq!(i.number.len(), 4);
        }
    }

    /// CON-102 negative-input: everything off-grammar is refused whole.
    #[test]
    fn malformed_codes_rejected() {
        for bad in [
            "",
            "1234",
            "1234-word",
            "1234-word-word-word",
            "123-word-word",     // too few digits
            "123456-word-word",  // too many digits
            "12a4-word-word",    // non-digit
            "1234-Word-word",    // uppercase
            "1234-ab-word",      // word too short
            "1234-abcdefghi-wd", // word too long
            "1234-wörd-word",    // non-ascii
            "1234--word",
            "-1234-word-word",
            "1234 word word",
        ] {
            assert!(parse(bad).is_err(), "must reject {bad:?}");
        }
    }

    /// Whitespace around a pasted code is tolerated; internal is not.
    #[test]
    fn surrounding_whitespace_tolerated() {
        assert!(parse("  1234-abandon-ability \n").is_ok());
        assert!(parse("1234-aban don-ability").is_err());
    }

    /// ADR-102: the rendezvous seed depends on the number ONLY. Two invites
    /// sharing a number share a rendezvous; the words never influence it.
    #[test]
    fn rendezvous_depends_only_on_routing_number() {
        let a = parse("1234-abandon-ability").unwrap();
        let b = parse("1234-zebra-zone").unwrap();
        let c = parse("5678-abandon-ability").unwrap();
        assert_eq!(a.rendezvous_seed(), b.rendezvous_seed());
        assert_ne!(a.rendezvous_seed(), c.rendezvous_seed());
    }

    /// The password is the secret half, and only that.
    #[test]
    fn password_is_the_two_words() {
        let i = parse("1234-abandon-ability").unwrap();
        assert_eq!(i.password(), "abandon-ability");
        assert!(!i.password().contains("1234"));
    }

    /// NFR-104: the wordlist gives ≥ 20 bits over two words.
    #[test]
    fn entropy_floor() {
        let n = words().len() as f64;
        let bits = 2.0 * n.log2();
        assert!(bits >= 20.0, "two words give {bits} bits, need ≥ 20");
    }

    /// Sanity: generated words come from the list.
    #[test]
    fn generated_words_are_in_the_list() {
        let list = words();
        for _ in 0..32 {
            let i = Invite::generate();
            assert!(list.contains(&i.w1.as_str()));
            assert!(list.contains(&i.w2.as_str()));
        }
    }
}
