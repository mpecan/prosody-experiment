# Examples

Nine poems you can actually compile. Unlike `crates/prosody-emit/tests/corpus/`, which holds
stripped fixtures chosen to pin specific behaviour, these are **verbatim texts** — original
punctuation, capitals, em-dashes and all — because the point is to find out what the mappings do
with real verse rather than with input shaped to suit them.

## Running them

```sh
# Strudel pattern text on stdout — paste it into https://strudel.cc
cargo run -p prosody-cli -- examples/blake-tyger.poem

# what each token became, and which pass decided it
cargo run -p prosody-cli -- --explain examples/wilde-theocritus.poem

# a schedule of OSC messages, in beats
cargo run -p prosody-cli -- --osc examples/dickinson-because.poem

# send it to SuperCollider and recompile on every save
cargo run -p prosody-cli -- --watch --send 127.0.0.1:57120 examples/blake-tyger.poem
```

To hear the OSC path you need a receiver: open `examples/prosody.scd` in SuperCollider, evaluate
it, then `--send 127.0.0.1:57120`. It defines three synths — one per §4.1 layer plus one for
§4.5's rhyme words — and schedules each note by the onset in its message rather than playing it on
arrival.

`./examples/run.sh` compiles all of them at once; `./examples/run.sh --explain` annotates them.

## What each one is for

| File | Exercises | Source |
|---|---|---|
| `shakespeare-sonnet-18.poem` | strict iambic pentameter; every stanza regular | Shakespeare, Sonnet 18 (1609) |
| `blake-tyger.poem` | trochaic tetrameter, and a first stanza that returns as the last | Blake, *Songs of Experience* (1794) |
| `dickinson-because.poem` | common measure — the alternating four-and-three §4.1's threshold is calibrated against | Dickinson (published 1890) |
| `whitman-song-of-myself.poem` | free verse: §9's degeneracy, where §4.1's fallback is all that gives the piece a pulse | Whitman, *Leaves of Grass* (1855) |
| `poe-annabel-lee.poem` | obsessive rhyme and near-refrain | Poe (1849) |
| `wilde-theocritus.poem` | a real villanelle — §4.5's "two-note obsession" | Wilde, *Theocritus: A Villanelle* (1881) |
| `hopkins-pied-beauty.poem` | alliteration at its densest, and sprung rhythm the scansion was never designed for | Hopkins, written 1877, published 1918 |
| `carroll-jabberwocky.poem` | invented words, so the letter-to-sound fallback carries most of the poem | Carroll, *Through the Looking-Glass* (1871) |
| `preseren-zdravljica.poem` | §9's language limitation: Slovenian through an English dictionary | Prešeren, *Zdravljica* (1844) |

Four carry a `[style]` header (§5) so the palette layer is exercised too; the rest take the
default.

All nine were first published before 1929 and their authors died long before that, so they are
public domain worldwide. The Prešeren text is transcribed from memory of a very widely reproduced
poem and may contain small errors; it is here to exercise the non-English path, not as an
authoritative edition.

## Does any of this actually make a sound?

Checked rather than assumed, because "it compiles" and "it plays" are different claims.

**Strudel: yes, verified.** The emitted text was evaluated in real `@strudel/core` and
`@strudel/mini` — not just parsed as JavaScript — and it produces genuine control events. Every
method the emitter calls (`gain`, `lpf`, `attack`, `room`, `crush`, `vowel`, `bank`, `note`, `s`,
`cpm`, `swingBy`, `stack`) exists on Strudel's `Pattern`. Querying Dickinson cycle by cycle gives
eight events in the first cycle and six in the second — which is the syllable count of "Because I
could not stop for Death" and "He kindly stopped for me". Successive cycles differ, so the poem's
lines really do play in order, and repeated queries give identical results. All five bank names
(`tr909`, `tr808`, `tr707`, `linn`, `casio`) resolve to real entries in Strudel's default
drum-machine sample set.

**OSC: yes, once you run the receiver.** The messages were decoded with `python-osc` — a
third-party implementation, not the one that wrote them — and all of them parse. `examples/`
ships the SuperCollider side, so there is something to listen.

That check found a real defect. The backend computed exact onsets, printed them in the text
output, and then **dropped them on the way to the wire**: every message arrived as fast as UDP
would carry it with no indication of when it belonged. It was invisible to every test in the
repository, because the schedule was only ever inspected through the same crate that built it.
Decoding the bytes with something else is what showed it. The onset is now the first argument of
every note, and the tempo is sent once ahead of them.

## What they show, including the parts that are wrong

Running these is how you find out whether the mappings hold up, so here is what they actually do
— the good and the bad.

**Blake gives 24 bars and 21 voices.** The three-voice difference is the first stanza returning as
the last: `SPEC.md` §4.3's refrain row, so those lines are *the same instrument played again*
rather than a lookalike. Six lines come back marked `refrain of voice`. Every stanza classifies as
`foundation (regularity)`, which is right — it is about as metrically regular as English gets.

**Whitman gives the opposite**, and correctly: six lines `flourish (irregularity)`, three
`foundation (mostregular)`. Nothing is regular enough to anchor on its own evidence, so §9's
fallback promotes the least ragged stanza and *says* it did. Only two events in the whole passage
carry a pitch, because free verse gives the rhyme pass almost nothing to work with.

**Jabberwocky marks nineteen words guessed**, Prešeren fifteen. Both still compile and play — the
letter-to-sound fallback carries them — and the debug view labels every guessed syllable, which is
`CLAUDE.md` §6's rule that a fallback must be visible.

**Wilde's villanelle does not do what §4.5 predicts, and that is the most useful thing here.**
A villanelle has exactly two rhymes. The pipeline finds *four* classes:

```
Persephone  deg 0        bee, sea            deg 2
Sicily      deg 1        gate, fate, state   deg 3
```

Every reader hears `Persephone / Sicily / bee / sea` as one rhyme. The rhyme pass splits them
because it takes the tail from the last **primary-stressed** vowel, and in `Persephone` that is
three syllables before the sound the rhyme actually turns on. The rule is right for monosyllables
and for masculine rhyme, and wrong for exactly the polysyllabic feminine rhyme a villanelle is
built out of. It is recorded in `PLAN.md`'s open questions.

The synthetic villanelle in `tests/corpus/` *does* collapse to two notes — because its rhyme words
are monosyllables. That is the difference between a fixture and a poem, and the reason this
directory exists.
