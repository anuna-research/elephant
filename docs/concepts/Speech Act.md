# Speech Act

An utterance that *does* something rather than merely describing —
Austin/Searle's performatives: asserting, promising, requesting,
conceding. [[Elephant 2000]]'s thesis is that program I/O should consist
of speech acts with real correctness conditions: assertions should be
truthful, answers responsive, promises kept.

In elephant every speech act is a signed [[CBCL]] message using a
[[cbcl-elephant]] performative, appended forever to a theory's [[Corpus]].
Its illocutionary force is the performative; its propositional content is
the [[SPL]] payload; its correctness conditions are evaluated by
[[Defeasible Logic]] closure (e.g. a [[Commitment]] deriving `fulfilled`
or `violated`).
