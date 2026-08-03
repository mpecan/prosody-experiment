# The golden corpus

`CLAUDE.md` §7.2 names seven poems this corpus must contain, one for each way the pipeline can
go wrong. Every one is either long out of copyright or written for this repository.

| File | Exercises | Source |
|---|---|---|
| `pentameter.poem` | strict iambic pentameter — the metre §4.1's threshold is calibrated against | Shakespeare, Sonnet 18 (1609) |
| `free-verse.poem` | §9's degeneracy: no regular stanza, so §4.1's fallback fires and §4.5 finds no rhyme | Whitman, *Song of Myself* (1855) |
| `villanelle.poem` | §4.5's "two-note obsession" and §4.3's refrain row at their densest | written for this repository |
| `refrain.poem` | a refrain in otherwise-unrelated lines — the case stanza-grain variance cannot see | Psalm 136, KJV (1611) |
| `one-line.poem` | the minimum viable input §9 asks about, and `StanzaBasis::OnlyCandidate` | written for this repository |
| `nonsense.poem` | invented words, so the letter-to-sound fallback carries most of the poem | Carroll, *Jabberwocky* (1871) |
| `slovenian.poem` | §9's language limitation, with a style header so the snapshot covers §5 too | written for this repository |

Punctuation and capitalisation are stripped throughout. The tokeniser keeps `'` inside a word
(`PLAN.md` records why), and a corpus full of scare quotes would be testing that decision rather
than the mappings.

## What the snapshots are for

Not "the output is what it is". A snapshot catches the class of regression that unit tests keep
missing here: a whole *column* going empty. Iteration 12's review found the palette block, the
header counts and the layer labels could all be blanked with the suite still green, and iteration
13 found the beat position could be a constant. One committed snapshot of the whole emitted text
catches every one of those at once, because everything is in it.

`CLAUDE.md` §7.2: "Review every snapshot diff by reading it — accepting an `insta` diff without
reading it is the main way a mapping silently rots."
