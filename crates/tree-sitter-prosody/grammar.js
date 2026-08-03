/**
 * A poem, as a score. See SPEC.md §7.1.
 *
 * The grammar's one structural job is to make the units the analysis passes need — stanza, line,
 * caesura, token — first-class nodes with byte ranges, so invariant I4 has something to attribute
 * to and invariant I2 has a unit to seed from.
 *
 * Newlines are deliberately NOT in `extras`. A blank line separates stanzas and a single newline
 * separates lines, so line structure is the grammar's business and must not be whitespace-skipped.
 * The stanza break is lexed as a single token rather than a repeat of blank lines, which is what
 * keeps the top level unambiguous.
 */
module.exports = grammar({
  name: 'prosody',

  // Horizontal whitespace only. Newlines carry meaning.
  extras: () => [/[ \t]+/],

  rules: {
    source_file: ($) =>
      seq(
        optional($.frontmatter),
        optional($._leading_space),
        optional(seq($.stanza, repeat(seq($._stanza_break, optional($.stanza))))),
      ),

    // Palette and constraint only (SPEC.md §5, invariant I3). Opaque here: the contents are TOML
    // and belong to the style layer. Lexed as one token so a stray delimiter inside the block
    // cannot desynchronise the poem body.
    frontmatter: () =>
      token(
        seq(
          '+++',
          // Trailing horizontal whitespace after a delimiter is a common editor artefact, and
          // rejecting it used to make the whole block lex as poem text — a silent I3 violation.
          /[ \t]*\r?\n/,
          repeat(choice(/[^+]/, /\+[^+]/, /\+\+[^+]/)),
          '+++',
          optional(/[ \t]*\r?\n/),
        ),
      ),

    stanza: ($) => prec.right(repeat1($.line)),

    // Right-associative because the trailing newline is optional: without this the parser
    // cannot tell a line that ends at EOF from one that continues.
    line: ($) => prec.right(seq(repeat1($._line_item), optional(/\r?\n/))),

    _line_item: ($) => choice($.number, $.caesura, $.word, $.punctuation),

    // A mid-line pause. SPEC.md §4.1 wants this available as a voice-splitting boundary; whether
    // it actually splits a voice is a later decision, not the grammar's.
    caesura: () => choice(',', ';', ':', token(prec(1, '--')), '—', '–'),

    // Digits grouped with internal separators, so a thousands separator or a clock time is one
    // token rather than a spurious caesura: "1,000" and "3:14" would otherwise each read as a
    // mid-line pause and split a voice at a number.
    number: () => token(/\d+([.,:]\d+)*/),

    // Letters from any script, so the Slovenian and multilingual cases of SPEC.md §9 tokenise even
    // though only English can be phonemised. Internal apostrophes and hyphens stay inside the
    // word: "burn'd" and "half-light" are one token each.
    word: () => /[^\s,;:.!?—–"“”()\[\]]+/,

    // An ellipsis is one mark, not three full stops.
    punctuation: () => choice(token(prec(1, '...')), /[.!?"“”()\[\]]/),

    // Two or more newlines, with any horizontal whitespace between them. Written as
    // "one, then one-or-more" rather than with a `{2,}` quantifier: tree-sitter lowers `{2,}`
    // into repeated *pairs*, which silently leaves an odd trailing newline unconsumed.
    _stanza_break: () => token(/\r?\n[ \t]*(\r?\n[ \t]*)+/),

    _leading_space: ($) => choice($._stanza_break, /\r?\n/),
  },
});
