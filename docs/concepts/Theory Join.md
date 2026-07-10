# Theory Join

The ceremony that admits a new member to a [[Logical Theory]]
([[SPEC-002-elephant-p2p#REQ-104]]): inviter mints a single-use
`num-word-word` code; the number derives a rendezvous [[pkarr]] record
(public routing), the words are the [[SPAKE2]] password (secret); after
key confirmation the inviter sends the sealed introduction (theory id,
genesis, [[DID Document]], roster), asserts a signed
`(member <did> <node-pk>)` fact into the [[Corpus]], and the joiner
completes initial [[Loro]] sync. Wrong guess burns the invite; failures
are remotely opaque. Pattern inherited from [[SPEC-047]].
