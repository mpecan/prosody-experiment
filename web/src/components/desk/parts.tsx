// The desk's hardware. Every knob and fader is a real range input underneath (turned by dragging
// across it, or Tab and the arrow keys); its 0–1 position is --v, which draws the arc, the pointer
// and the meter.
import type { ComponentChildren } from 'preact';

/** Where `value` sits between `min` and `max`, 0–1. */
export function position(value: string | number, min: number, max: number): number {
  const at = (Number(value) - min) / (max - min);
  return Math.max(0, Math.min(1, at || 0));
}

interface Range {
  id: string;
  label: string;
  value: string | number;
  min: number;
  max: number;
  step: number;
  onInput: (value: string) => void;
  /** The layer colour, a CSS colour or variable. */
  colour: string;
  title?: string;
  /** What the readout shows; the value by default. */
  shown?: string;
}

const v = (value: string | number, min: number, max: number) => position(value, min, max).toFixed(4);
/** A value as a range input reads it back: `2.0` is `2`, `0.30` is `0.3`. */
const read = (value: string | number) => (value === '' || isNaN(Number(value)) ? String(value) : String(Number(value)));

/** A knob: a lit arc of 270°, a cap, and a pointer. */
export function Knob({ id, label, value, min, max, step, onInput, colour, title, shown }: Range) {
  return (
    <label class="knob" style={{ '--c': colour, '--v': v(value, min, max) }} title={title}>
      <span class="dial">
        <input id={id} type="range" min={min} max={max} step={step} value={value} aria-label={label}
          onInput={(event) => onInput(event.currentTarget.value)} />
        <i />
      </span>
      <span class="engrave">{label}</span>
      <output id={id + 'Out'}>{shown ?? read(value)}</output>
    </label>
  );
}

/** A fader: a rail with a meter beside it, the cap in its layer colour. */
export function Fader({ id, label, value, min, max, step, onInput, colour, title, engraved }: Range & { engraved: string }) {
  return (
    <label class="fader" style={{ '--c': colour, '--v': v(value, min, max) }} title={title}>
      <span class="rail">
        <span class="meter" />
        <input id={id} type="range" min={min} max={max} step={step} value={value} aria-label={label}
          onInput={(event) => onInput(event.currentTarget.value)} />
      </span>
      <output id={id + 'Out'}>{read(value)}</output>
      <span class="engrave">{engraved}</span>
    </label>
  );
}

/** A selector: a menu in a dark well. */
export function Selector({ id, label, value, options, onChange, title, inline = false }: {
  id: string;
  label: string;
  value: string;
  options: { value: string; label: string; disabled?: boolean }[];
  onChange: (value: string) => void;
  title?: string;
  inline?: boolean;
}) {
  return (
    <label class={inline ? 'selector inline' : 'selector'} title={title}>
      <span class="engrave">{label}</span>
      <select id={id} value={value} onChange={(event) => onChange(event.currentTarget.value)}>
        {options.map((option) => <option key={option.value} value={option.value} disabled={option.disabled}>{option.label}</option>)}
      </select>
    </label>
  );
}

/** A plain list of values as a selector's options. */
export const plain = (values: string[]) => values.map((value) => ({ value, label: value }));

/** A flip switch: a lever that lights when on. */
export function Switch({ id, checked, onChange, children }: {
  id: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  children: ComponentChildren;
}) {
  return (
    <label class="switch">
      <input id={id} class="flip" type="checkbox" checked={checked} onChange={(event) => onChange(event.currentTarget.checked)} />
      <span class="engrave">{children}</span>
    </label>
  );
}
