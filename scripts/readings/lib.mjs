// The pure half of the recording tool: which lines to record, with which voices, at what pace, and
// the manifest that says what was recorded. No IO here, so it can be tested on its own
// (`npm test`); render.mjs does the compiling, the speaking and the files.

import { createHash } from 'node:crypto';

// The voices the examples are recorded in (SPEC.md §4.7, the `recorded` engine). Each is a voice
// of one of the page's live engines, rendered by that engine's own code (speech-worker.mjs), so a
// line with no recording can be read live in the same voice. Chosen for licences that allow the
// recordings to be published with the site, and for being different from each other: two Kokoro
// voices (American and British, female and male) and one Piper voice for English; Piper's only
// Slovenian voice for Prešeren. README "Voices and their licences" has the whole of each licence.
export const VOICES = [
  {
    id: 'kokoro:af_heart', engine: 'kokoro', voice: 'af_heart', lang: 'en',
    name: 'Heart — Kokoro-82M, American English, female',
    licence: 'Kokoro-82M v1.0 (onnx-community/Kokoro-82M-v1.0-ONNX), Apache-2.0',
  },
  {
    id: 'kokoro:bm_george', engine: 'kokoro', voice: 'bm_george', lang: 'en',
    name: 'George — Kokoro-82M, British English, male',
    licence: 'Kokoro-82M v1.0 (onnx-community/Kokoro-82M-v1.0-ONNX), Apache-2.0',
  },
  {
    id: 'piper:en_GB-cori-medium', engine: 'piper', voice: 'en_GB-cori-medium', lang: 'en',
    name: 'Cori — Piper, British English, female',
    licence: 'rhasspy/piper-voices (MIT) en_GB-cori-medium, trained on public-domain LibriVox recordings',
  },
  {
    id: 'piper:sl_SI-artur-medium', engine: 'piper', voice: 'sl_SI-artur-medium', lang: 'sl',
    name: 'Artur — Piper, Slovenian, male',
    licence: 'rhasspy/piper-voices (MIT) sl_SI-artur-medium, trained on the ARTUR corpus (ppisljar/artur_studio_tts), CC BY 4.0',
  },
];

// The settings a reading is asked for under, so every line any of them can speak is recorded: the
// default (`breath`), the other variants, and without the outro, which can leave a stanza unread.
export const SETTINGS = ['', 'set=reading:arrives', 'set=reading:stanza', 'off=outro', 'off=outro&set=reading:arrives'];

// The audio: MP3, mono, at the voice's own sample rate (24 kHz for Kokoro, 22.05 kHz for Piper),
// 32 kbit/s. MP3 rather than Opus because every browser decodes it with `decodeAudioData`, Safari
// included, and the browsers the recordings are most for are the ones the live engines fail in.
export const AUDIO = { format: 'mp3', bitrate: '32k', describe: 'MP3, mono, 32 kbit/s, at the voice\'s sample rate' };

// A line's key: the first 16 hex digits (64 bits) of the SHA-256 of its exact text in UTF-8. An
// edited line has another key, so it simply has no recording.
export function keyOf(text) {
  return createHash('sha256').update(text, 'utf8').digest('hex').slice(0, 16);
}

// Where a recording lives under the readings directory: its voice, its text's key and the pace it
// was spoken at, to the manifest's three places. The pace is part of the name because it is baked
// into the audio: a pace rule changed with no line's text changed then wants other files, so the
// old ones are left over (and removed) and the new ones missing (and rendered), and `--check`
// fails until they are. Named by text alone, the old audio would be kept and the manifest
// rewritten to claim the new pace for it — played as if at a speed it was not spoken at.
export function fileOf(voiceId, key, pace) {
  return `${voiceId.replace(':', '-')}/${key}-${Number(pace).toFixed(3)}.${AUDIO.format}`;
}

// The lines a poem's readings speak, from the `/reading` answers under every setting: text → pace.
// A line read under two settings is the same line at the same pace, its pace being drawn from its
// own text (SPEC.md §4.7, "natural"); a disagreement is a bug, and thrown.
export function linesOf(answers) {
  const lines = new Map();
  for (const answer of answers) {
    for (const line of [...answer.lines, ...answer.cues]) {
      if (!line.text.trim()) continue;
      const known = lines.get(line.text);
      if (known !== undefined && known !== line.pace) {
        throw new Error(`"${line.text}" is read at ${known} and at ${line.pace}`);
      }
      lines.set(line.text, line.pace);
    }
  }
  return lines;
}

// Every recording wanted: for each poem, each line, each voice of the poem's language.
export function wanted(poems, voices = VOICES) {
  const out = [];
  for (const poem of poems) {
    for (const voice of voices.filter((v) => v.lang === poem.language)) {
      for (const [text, pace] of poem.lines) {
        const key = keyOf(text);
        out.push({ poem: poem.name, voice, text, pace, key, file: fileOf(voice.id, key, pace) });
      }
    }
  }
  return out;
}

// The manifest, as text: deterministic (every map sorted by key, numbers to three places), one
// voice and one line to a line of the file, so a diff of it reads and the Rust test can check it
// line by line. `recorded` maps a wanted recording's file to { seconds }, for those that exist;
// `devices` maps a voice id to what it was rendered on.
export function manifest(poems, recordings, recorded, devices, voices = VOICES) {
  const json = (value) => JSON.stringify(value);
  const fixed = (n) => Number(n).toFixed(3);
  const used = voices.filter((v) => recordings.some((r) => r.voice.id === v.id && recorded.has(r.file)));
  const voiceLines = used.map((v) => `    ${json(v.id)}: {"engine":${json(v.engine)},"voice":${json(v.voice)},"lang":${json(v.lang)},"name":${json(v.name)},"device":${json(devices[v.id] ?? 'unknown')},"licence":${json(v.licence)}}`);
  const byKey = new Map();
  for (const r of recordings) {
    if (!recorded.has(r.file)) continue;
    const entry = byKey.get(r.key) ?? { text: r.text, pace: r.pace, files: new Map() };
    entry.files.set(r.voice.id, { file: r.file, seconds: recorded.get(r.file).seconds });
    byKey.set(r.key, entry);
  }
  const lineLines = [...byKey.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)).map(([key, entry]) => {
    const files = [...entry.files.entries()].sort(([a], [b]) => (a < b ? -1 : 1))
      .map(([id, f]) => `${json(id)}:{"file":${json(f.file)},"seconds":${fixed(f.seconds)}}`);
    return `    ${json(key)}: {"text":${json(entry.text)},"pace":${fixed(entry.pace)},"recordings":{${files.join(',')}}}`;
  });
  const poemLines = [...poems].sort((a, b) => (a.name < b.name ? -1 : 1))
    .map((p) => `    ${json(p.name)}: {"language":${json(p.language)},"lines":${p.lines.size}}`);
  return [
    '{',
    '  "version": 1,',
    `  "audio": ${json(AUDIO.describe)},`,
    '  "key": "the first 16 hex digits of the SHA-256 of the line\'s exact text in UTF-8",',
    '  "voices": {',
    voiceLines.join(',\n'),
    '  },',
    '  "poems": {',
    poemLines.join(',\n'),
    '  },',
    '  "lines": {',
    lineLines.join(',\n'),
    '  }',
    '}',
    '',
  ].join('\n');
}
