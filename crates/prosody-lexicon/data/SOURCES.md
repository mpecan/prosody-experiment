# Where the rarity table comes from

`rarity.txt` holds word-frequency *bands* derived from fourteen long-public-domain English works,
counted together:

| work | author | kind |
|---|---|---|
| Sonnets | Shakespeare | verse |
| Songs of Innocence and of Experience | Blake | verse |
| Leaves of Grass | Whitman | verse |
| Poems | Keats | verse |
| Poetical Works | Coleridge | verse |
| Poems | Dickinson | verse |
| North of Boston | Frost | verse |
| Responsibilities | Yeats | verse |
| Men and Women | Browning | verse |
| King James Bible | — | prose |
| A Tale of Two Cities | Dickens | prose |
| Pride and Prejudice | Austen | prose |
| The Call of the Wild | London | prose |
| The Adventures of Sherlock Holmes | Doyle | prose |

1,909,419 tokens, 43,119 distinct words; the commonest 20,000 are kept in log-spaced bands and the
rest are treated as maximally rare by their absence.

Only the derived counts are distributed, never the texts. Every work is long out of copyright, and
a frequency count is a fact about a text rather than an expressive part of it.

## Each source counts equally

A source contributes its own *relative* frequencies, averaged with equal weight, rather than its
raw counts. The King James Bible alone is four fifths of the bytes; counted raw it decided the
whole table, and measurement showed the result: "thou" and "unto" ranked alongside "it" and "not",
while `coffee`, `hemlock`, `dryad` and `gossamer` all collapsed into the same maximally-rare bucket
as invented gibberish — including words drawn from Keats, one of the sources.

Prose was added for the same reason. Verse alone knows "dryad" and not "kitchen".

## Why a public-domain corpus rather than a web corpus

Two reasons, and the second is the one that matters.

It is the licensable option: the widely-used Google-derived frequency lists carry Linguistic Data
Consortium terms that do not permit redistribution.

And it is the right corpus. `SPEC.md` §4.4 wants lexical rarity to drive transient sharpness — so
what should sound striking in a poem is a word that is rare *in poems*. Web frequencies would call
"thee", "thy" and "hath" exotic and "click" ordinary, which is backwards for this instrument.

## What it cannot know

Everything is pre-1929, so genuinely modern vocabulary — `phone`, `jazz`, `explode`, `plastic` —
is absent and scores as maximally rare. For a poem about a mobile phone that is wrong, and no
public-domain corpus can fix it. Recorded in `PLAN.md` rather than papered over.
