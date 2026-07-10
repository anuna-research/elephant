# MLS

Messaging Layer Security (RFC 9420): continuous group key agreement with
forward secrecy and post-compromise security, epochs advanced by
commits, new members admitted via KeyPackage + Welcome. elephant embeds
[openmls](https://github.com/openmls/openmls) 0.8 (RustCrypto provider,
basic credentials carrying the member's [[DID]]) — the same integration
pattern as `../hark`.

elephant's use ([[SPEC-004-elephant-e2ee]]): one MLS group per
[[Logical Theory]] (group_id = theory id); the steward (creator) is sole
committer; commits travel in the theory doc's `mls` lane; Welcome rides
the [[SPAKE2]] join channel. Corpus entries are sealed under random data
keys distributed as MLS application messages (the keybook), so admitted
members can read the full history — MLS's own forward secrecy governs
the keys, not the archive — while removal rotates the data key and locks
the removed member out of everything that follows.
