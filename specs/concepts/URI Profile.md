---
title: URI Profile
mode: reference
---

# URI Profile

The Elephant profile is the restricted URI language in [[SPEC-007-theory-references#CON-601]].
It identifies a theory without an authority component or network address.
An optional `sentence=` fragment selects a signed statement in the Elephant theory representation.

| Source | Governing fact |
|---|---|
| [RFC 3986 §3](https://www.rfc-editor.org/rfc/rfc3986.html#section-3) | URI syntax permits a scheme followed by a rootless path; `//` is optional. |
| [RFC 3986 §3.1](https://www.rfc-editor.org/rfc/rfc3986.html#section-3.1) | Scheme names are case-insensitive; their canonical spelling is lowercase. |
| [RFC 3986 §3.5](https://www.rfc-editor.org/rfc/rfc3986.html#section-3.5) | Fragments identify secondary resources; their semantics depend on the representation, not the scheme. |
| [RFC 7405](https://www.rfc-editor.org/rfc/rfc7405.html) | `%s` marks case-sensitive ABNF string literals. |
| [IANA URI scheme registry](https://www.iana.org/assignments/uri-schemes/uri-schemes.xhtml) | Registry consulted for scheme-name status; syntax validity alone does not establish registration. |

The syntax is a proposed application contract, not a registered protocol claim.
The representation interprets `sentence=<sid>` as selecting the entry resolved by [[SPEC-007-theory-references#CON-603]].
Both text and JSON views identify that same entry.
No MIME media-type registration is claimed.

Crawl evidence: `specs/evidence/theory-linking/rfc3986.json` and `iana.json`.
Standards evidence and its retrieval status are recorded in [[theory-linking-review]].
