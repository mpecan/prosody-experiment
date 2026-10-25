// Who reads a line: the chosen engine once it can, the recording where there is one, and otherwise
// whoever can stand in — said, never silent.
import { beforeEach, describe, expect, it } from 'vitest';

import { engines, readSaved, speaker, speakerOf, voiceOf } from '../src/reading';
import { language, readingSettings } from '../src/store';
import { useFixture } from './site.fixture';

const RECORDED = 'Tyger Tyger, burning bright,';

beforeEach(() => {
  useFixture();
  language.value = 'en';
  readingSettings.value = { engine: 'recorded', pace: 'natural', level: 1, voices: {} };
  const recorded = engines.recorded;
  recorded.state = 'ready';
  recorded.manifest = {
    voices: { 'kokoro:af_heart': { name: 'Heart', lang: 'en-US' } },
    lines: { abc: { text: RECORDED, pace: 1.06, recordings: { 'kokoro:af_heart': { file: 'kokoro-af_heart/abc.mp3', seconds: 1.5 } } } },
  };
  recorded.byText = new Map([[RECORDED, { key: 'abc', ...recorded.manifest.lines.abc! }]]);
  engines.kokoro.state = 'idle';
  engines.kokoro.listed = null;
});

describe('the recorded readings', () => {
  it('read a line from its file, in the voice chosen', () => {
    expect(speakerOf(RECORDED)).toMatchObject({ engine: 'recorded', voice: 'kokoro:af_heart', why: '' });
  });

  it('hand an edited line to the browser\'s voice while the voice that made them has not loaded', () => {
    expect(speakerOf('Tyger Tyger, burning brightly,')).toEqual({
      engine: 'browser', voice: null, why: 'no recording of it; read by the browser voice', fallback: true,
    });
  });

  it('hand it to the live voice that made them once that has loaded — never downloading it unasked', () => {
    engines.kokoro.state = 'ready';
    engines.kokoro.listed = [{ id: 'af_heart', name: 'Heart', lang: 'en-us' }];
    expect(speakerOf('Tyger Tyger, burning brightly,')).toMatchObject({
      engine: 'kokoro', voice: 'af_heart', fallback: true,
      why: 'no recording of it; read live by Kokoro-82M af_heart',
    });
  });
});

describe('a neural engine', () => {
  it('is stood in for by the browser\'s voice until it has loaded, and says so', () => {
    readingSettings.value = { ...readingSettings.value, engine: 'kokoro' };
    expect(speaker()).toEqual({ engine: 'browser', why: 'Kokoro-82M not loaded; reading with the browser voice meanwhile' });
    engines.kokoro.state = 'failed';
    engines.kokoro.error = 'no network';
    expect(speaker()).toMatchObject({ engine: 'browser', bad: true });
  });

  it('with no voice in the poem\'s language is said to have none, and Piper named for Slovenian', () => {
    readingSettings.value = { ...readingSettings.value, engine: 'kokoro' };
    engines.kokoro.state = 'ready';
    engines.kokoro.listed = [{ id: 'af_heart', name: 'Heart', lang: 'en-us' }];
    language.value = 'sl';
    expect(speaker().why).toBe('Kokoro-82M has no Slovenian voice; reading with the browser voice (Piper has one)');
  });

  it('reads with the voice saved, else its default, else its first', () => {
    engines.kokoro.listed = [{ id: 'am_adam', name: 'Adam', lang: 'en-us' }, { id: 'af_heart', name: 'Heart', lang: 'en-us' }];
    expect(voiceOf('kokoro')).toBe('af_heart');
    readingSettings.value = { ...readingSettings.value, voices: { kokoro: { en: 'am_adam' } } };
    expect(voiceOf('kokoro')).toBe('am_adam');
    engines.kokoro.listed = [{ id: 'bf_emma', name: 'Emma', lang: 'en-gb' }];
    expect(voiceOf('kokoro')).toBe('bf_emma');
  });
});

describe('the reading\'s saved settings', () => {
  it('start with the recorded readings at the natural pace: nothing downloaded unasked', () => {
    expect(readSaved(null, null)).toEqual({ engine: 'recorded', pace: 'natural', level: 1, voices: {} });
  });

  it('keep what they know and drop what they do not', () => {
    expect(readSaved({ engine: 'piper', pace: 'fitted', level: 1.5, voices: { piper: { sl: 'x' } } }, null))
      .toEqual({ engine: 'piper', pace: 'fitted', level: 1.5, voices: { piper: { sl: 'x' } } });
    expect(readSaved({ engine: 'espeak', pace: 'slow', level: 7 }, null))
      .toEqual({ engine: 'recorded', pace: 'natural', level: 1, voices: {} });
  });

  it('carry over the browser voice picked before there were engines', () => {
    expect(readSaved(null, 'Samantha').voices.browser).toEqual({ en: 'Samantha' });
  });
});
