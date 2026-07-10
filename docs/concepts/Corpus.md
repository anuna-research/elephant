# Corpus

The append-only history of one [[Logical Theory]]: a [[Loro]] list of
Entries ([[SPEC-001-elephant-core#CON-002]]), each a signed, canonical
[[CBCL]] speech-act message with an HLC timestamp. It is McCarthy's
"history list"/journal ([[Elephant 2000]] thesis 3): programs refer
directly to the past, so nothing is ever deleted — retraction
([[SPEC-001-elephant-core#REQ-006]]) and membership are *new* entries
whose effect is computed at closure time. The corpus is the single
source of truth; conclusions, commitments, rosters, and task states are
all pure functions of it.
