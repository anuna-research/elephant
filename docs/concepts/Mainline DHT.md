# Mainline DHT

The BitTorrent Kademlia distributed hash table — ~10M nodes, the largest
deployed DHT — used by [[pkarr]] (BEP-44 mutable items) to store signed
DNS packets under Ed25519 keys. elephant never stores corpus data there:
only rendezvous and discovery *hints* ([[SPEC-002-elephant-p2p#CON-104]]).
Records expire within hours and are republished by the [[Daemon]];
resolution confers no trust ([[SPEC-002-elephant-p2p#REQ-107]]).
