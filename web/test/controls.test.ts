import { beforeEach, describe, expect, it } from 'vitest';

import { choosePalette, paletteOf, setControl, startControls } from '../src/controls';
import { controls } from '../src/store';
import { useFixture } from './site.fixture';

beforeEach(() => {
  useFixture();
  startControls();
});

describe('the palette menu', () => {
  it('names the palette the controls match', () => {
    expect(paletteOf(controls.value)).toBe('default');
  });

  it('matches numbers as numbers, as a range input reads them back', () => {
    // A slider reads `0.30` back as `0.3`: still the default palette.
    setControl('compress', '0.3');
    expect(paletteOf(controls.value)).toBe('default');
  });

  it('says custom once a control leaves every palette', () => {
    setControl('reverb', '0.6');
    expect(paletteOf(controls.value)).toBe('custom');
  });

  it('sets only a palette\'s own keys: jazz its scale, swing and bank too, never the root or tempo', () => {
    setControl('root', 'F#');
    setControl('low', '90');
    expect(choosePalette('jazz')).toBe(true);
    expect(controls.value.scale).toBe('dorian');
    expect(controls.value.swing).toBe('0.33');
    expect(controls.value.bank).toBe('linn');
    expect(controls.value.kit).toBe('jazz');
    expect(controls.value.root).toBe('F#');
    expect(controls.value.low).toBe('90');
    expect(paletteOf(controls.value)).toBe('jazz');
    expect(choosePalette('polka')).toBe(false);
  });
});
