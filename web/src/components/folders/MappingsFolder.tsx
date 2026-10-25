// The Mappings folder: what happens. Every candidate mapping is a selector, set for A and for B;
// switch between them while it plays, and Under the hood measures both. A row where A and B differ
// glows, since that is what the A/B switch lets you hear.
import { useState } from 'preact/hooks';

import { site } from '../../data';
import { schedule } from '../../piece';
import { choose, putBack } from '../../settings';
import { settings, suggested, type Side } from '../../store';
import { nameOf } from '../../names';
import type { MappingInfo } from '../../site';
import { More } from '../desk/More';
import { Segmented } from '../desk/Segmented';

const SIDES: [Side, string][] = [['A', 'var(--pulse)'], ['B', 'var(--ornament)']];

function Row({ mapping }: { mapping: MappingInfo }) {
  const [open, setOpen] = useState(false);
  const { name, what, variants, default: fallback } = mapping;
  const chosen = (side: Side) => settings.value[side][name] ?? fallback;
  const differ = chosen('A') !== chosen('B');
  return (
    <li class={differ ? 'mapping differ' : 'mapping'} data-name={name}>
      <div class="mapping-head">
        <code>{name}</code>
        <button type="button" class="why" aria-expanded={open} aria-label={`What ${name} does`} onClick={() => setOpen(!open)}>?</button>
        {suggested.value[name] && <span class="tag">set on A by {nameOf(suggested.value[name].style)}</span>}
        <span class="flag">{differ ? 'A and B differ' : 'same on both'}</span>
      </div>
      {open && <p class="what">{what}</p>}
      {SIDES.map(([side, colour]) => (
        <div class="side" key={side}>
          <span class="side-name" style={{ color: colour }}>{side}</span>
          <Segmented label={`${name}, setting ${side}`} showLabel={false} value={chosen(side)} colour={colour} marked={fallback}
            options={variants.map((v) => ({ value: v, label: v }))}
            onChange={(variant) => { choose(side, name, variant); schedule(); }} />
        </div>
      ))}
    </li>
  );
}

export function MappingsFolder() {
  const differing = site.mappings.filter(({ name, default: fallback }) =>
    (settings.value.A[name] ?? fallback) !== (settings.value.B[name] ?? fallback)).length;
  return (
    <div class="desk">
      <div class="desk-head">
        <h2>Mappings</h2>
        <span class="hint">{differing === 1 ? '1 setting differs between A and B.' : `${differing} settings differ between A and B.`}</span>
      </div>
      {Object.keys(suggested.value).length > 0 && (
        <div class="suggested">
          <p>A style set {Object.keys(suggested.value).length === 1 ? 'one of A\'s mappings' : `${Object.keys(suggested.value).length} of A's mappings`}, marked below. Change any of them, or:</p>
          <button type="button" id="putBack" onClick={() => { putBack(); schedule(); }}>Put A back as it was</button>
        </div>
      )}
      <ul class="mappings" id="mappings">
        {site.mappings.map((mapping) => <Row key={mapping.name} mapping={mapping} />)}
      </ul>
      <More label="About the mappings">
        <p>
          Candidate mappings still under review. Each is a selector: choose a variant — or
          {' '}<code>off</code> — for each setting, then switch between A and B while it plays; the Score
          view measures both. A dotted underline marks each mapping's default. The
          {' '}<code>pulse</code> has no off: <code>steady</code> gives every bar as long as its beats,
          {' '}<code>elastic</code> every bar the same span, as the page once played every poem. Nor
          has the <code>stack</code>: <code>crossing</code> keeps every line on its own feet,
          {' '}<code>straight</code> fits a line sounding under the bar's line to that line's
          subdivision wherever the two would cross.
        </p>
      </More>
    </div>
  );
}
