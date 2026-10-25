// A segmented switch, for a choice of a few: every option in view, the chosen one lit.
import type { Choice } from './Patch';

export function Segmented({ label, value, options, onChange, colour = 'var(--accent)', id, showLabel = true, marked, wrap = false }: {
  label: string;
  value: string;
  options: Choice[];
  onChange: (value: string) => void;
  colour?: string;
  id?: string;
  showLabel?: boolean;
  /** An option to mark as the default. */
  marked?: string;
  /** Separate keys that wrap onto more rows, for options too many or too long for one. */
  wrap?: boolean;
}) {
  return (
    <div class="segmented-field">
      {showLabel && <span class="label">{label}</span>}
      <div class={wrap ? 'segmented wrap' : 'segmented'} role="group" aria-label={label} id={id} style={{ '--c': colour }}>
        {options.map((option) => (
          <button key={option.value} type="button" aria-pressed={option.value === value} disabled={option.disabled}
            data-value={option.value} class={option.value === marked ? 'default' : undefined}
            title={option.value === marked ? `${option.label} (the default)` : undefined}
            onClick={() => onChange(option.value)}>
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}
