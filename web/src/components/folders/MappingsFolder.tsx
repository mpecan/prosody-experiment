// The Mappings folder: what happens. Every candidate mapping is a selector, set for A and for B;
// switch between them while it plays, and the Score view measures both.
import { site } from '../../data';
import { schedule } from '../../piece';
import { choose } from '../../settings';
import { settings, type Side } from '../../store';

function Choice({ side, name, variants, fallback }: { side: Side; name: string; variants: string[]; fallback: string }) {
  const chosen = settings.value[side][name] ?? fallback;
  return (
    <select data-side={side} data-name={name} aria-label={`${name}, setting ${side}`} value={chosen}
      class={chosen !== fallback ? 'changed' : undefined}
      onChange={(event) => { choose(side, name, event.currentTarget.value); schedule(); }}>
      {variants.map((v) => <option key={v} value={v}>{v}{v === fallback ? ' (default)' : ''}</option>)}
    </select>
  );
}

export function MappingsFolder() {
  return (
    <>
      <div class="desk-head"><h2>Mappings</h2><span class="engrave">what happens · set A and B, switch while it plays</span></div>
      <div class="palette mappings">
        <table>
          <thead><tr><th /><th>A</th><th>B</th></tr></thead>
          <tbody id="mappings">
            {site.mappings.map(({ name, what, variants, default: fallback }) => (
              <tr key={name}>
                <td><code>{name}</code> <span class="what">— {what}</span></td>
                <td class="check"><Choice side="A" name={name} variants={variants} fallback={fallback} /></td>
                <td class="check"><Choice side="B" name={name} variants={variants} fallback={fallback} /></td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          Candidate mappings still under review. Each is a selector: choose a variant — or
          {' '}<code>off</code> — for each setting, then switch between A and B while it plays; the Score
          tab measures both. A bold border marks a choice that is not the default. The
          {' '}<code>pulse</code> has no off: <code>steady</code> gives every bar as long as its beats,
          {' '}<code>elastic</code> every bar the same span, as the page once played every poem. Nor
          has the <code>stack</code>: <code>crossing</code> keeps every line on its own feet,
          {' '}<code>straight</code> fits a line sounding under the bar's line to that line's
          subdivision wherever the two would cross.
        </p>
      </div>
    </>
  );
}
