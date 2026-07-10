# SPAKE2

SPAKE2 is a password-authenticated key exchange (PAKE): two parties who share
a short low-entropy secret derive a strong shared key, and an active attacker
gets exactly one online guess per protocol run.

Elephant uses the RustCrypto `spake2` crate (v0.4, `Ed25519Group`,
`start_symmetric`) for the [[Theory Join]] ceremony: the invite code's secret
words are the password; a failed guess burns the invite. The 32-byte output
key is never used raw — it feeds an HKDF for key-confirmation MAC and for
sealing the theory's introduction payload (membership roster, corpus
bootstrap pointer).

Security floor (inherited from [[SPEC-047]]/Magic Wormhole analysis): two
BIP39 words ≈ 22 bits of entropy is adequate *only because* PAKE limits the
attacker to a single online guess and the invite is single-use with a short
TTL. The code's numeric prefix is routing, not secret — it derives the
[[pkarr]] rendezvous record and is deliberately assumed public.
