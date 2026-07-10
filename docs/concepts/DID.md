# DID

W3C Decentralized Identifier. elephant agents are identified by
`did:crdt` DIDs from [did-crdt](https://github.com/anuna-research/did-crdt)
(used with default features — pure core, no networking): the DID is
`did:crdt:<blake3-hex>` of the genesis delta; the [[DID Document]] is a
CRDT of signed deltas supporting multi-key, rotation and revocation.
elephant owns key generation (Ed25519, OS RNG) and custody
([[SPEC-001-elephant-core#CON-005]], 0600), signs corpus Entries with the
identity key, and ships the serialized Document to peers at
[[Theory Join]] so signatures verify offline. The DID is the unit of
trust: `(trusts agent:<name> w)` weights bind to signer DIDs.
