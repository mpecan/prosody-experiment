// A patch display: a value on a small dark screen, stepped through with ◀ and ▶ as on a synth.
// A real menu lies over the screen, so a click on it — or Tab to it and the keyboard, or a phone's
// own picker — chooses from the whole list.

export interface Choice {
  value: string;
  label: string;
  disabled?: boolean;
}

export function Patch({ id, label, value, options, onChange, colour = 'var(--accent)', title, disabled = false, hideLabel = false }: {
  id: string;
  label: string;
  value: string;
  options: Choice[];
  onChange: (value: string) => void;
  colour?: string;
  title?: string;
  disabled?: boolean;
  /** The label for screen readers only, where the place says what it is (the poem picker). */
  hideLabel?: boolean;
}) {
  const choosable = options.filter((option) => !option.disabled);
  const at = choosable.findIndex((option) => option.value === value);
  const shown = options.find((option) => option.value === value)?.label ?? value;
  const step = (by: number) => {
    if (!choosable.length) return;
    const next = choosable[((at < 0 ? 0 : at + by) + choosable.length) % choosable.length];
    if (next) onChange(next.value);
  };
  // Where it is in the list: a dot each for a short one, a count for a long one.
  const many = choosable.length > 12 || at < 0;
  return (
    <div class="patch" style={{ '--c': colour }} title={title}>
      <label class={hideLabel ? 'label visually-hidden' : 'label'} for={id}>{label}</label>
      <div class="patch-row">
        <button type="button" class="step" aria-label={`Previous ${label.toLowerCase()}`} disabled={disabled} onClick={() => step(-1)}>
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M8 2 L4 6 L8 10" /></svg>
        </button>
        <div class="lcd">
          <span class="lcd-text">{shown}</span>
          {many
            ? <span class="lcd-count">{at < 0 ? `${choosable.length} to choose from` : `${at + 1} / ${choosable.length}`}</span>
            : <span class="lcd-dots" aria-hidden="true">{choosable.map((option) => <i key={option.value} class={option.value === value ? 'on' : undefined} />)}</span>}
          <select id={id} value={value} disabled={disabled} onChange={(event) => onChange(event.currentTarget.value)}>
            {options.map((option) => <option key={option.value} value={option.value} disabled={option.disabled}>{option.label}</option>)}
          </select>
        </div>
        <button type="button" class="step" aria-label={`Next ${label.toLowerCase()}`} disabled={disabled} onClick={() => step(1)}>
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M4 2 L8 6 L4 10" /></svg>
        </button>
      </div>
    </div>
  );
}
