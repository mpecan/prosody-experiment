// A short "more" that opens to the longer explanation, so a folder leads with its controls.
import type { ComponentChildren } from 'preact';
import { useState } from 'preact/hooks';

export function More({ label, children }: { label: string; children: ComponentChildren }) {
  const [open, setOpen] = useState(false);
  return (
    <div class="more">
      <button type="button" class="more-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Less' : label}</button>
      {open && <div class="more-text">{children}</div>}
    </div>
  );
}
