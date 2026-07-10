# DID Document

The W3C DID Core document describing a [[DID]]'s verification methods and
services. In did-crdt it is materialised from a CRDT of signed deltas
(`Document::resolve()` → JSON-LD), so rotation and revocation are
themselves auditable history — the same never-forget principle as the
[[Corpus]]. elephant persists it at `identity/did.json` and includes it
in the [[Theory Join]] introduction payload; Entry verification resolves
the signer's key from the Document, never from the wire.
