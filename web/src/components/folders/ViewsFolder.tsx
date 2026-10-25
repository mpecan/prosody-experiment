// Under the hood: what the poem compiled to — the pattern heard, what each word became, and both
// settings measured side by side.
import { measure } from '../../piece';
import { explainView, patternView, poem, profile, scores, view, type Score, type View } from '../../store';

const VIEWS: [View, string, string][] = [
  ['pattern', 'tabPattern', 'Pattern'],
  ['explain', 'tabExplain', 'What each word became'],
  ['score', 'tabScore', 'Score A vs B'],
];

/** B − A, coloured by which way it went, `·` when it did not move. */
function Delta({ a, b }: { a: number | null | undefined; b: number | null | undefined }) {
  if (a == null || b == null) return <td />;
  const d = b - a;
  const shown = (d > 0 ? '+' : '') + d.toFixed(3);
  return <td class={d > 0.0005 ? 'up' : d < -0.0005 ? 'down' : undefined}>{Math.abs(d) < 0.0005 ? '·' : shown}</td>;
}

/** Each metric of both settings, then the totals. A value outside its band is marked. */
export function ScoreRows({ measured }: { measured: { A: Score | null; B: Score | null } | 'measuring' }) {
  if (measured === 'measuring') return <tr><td colSpan={5}>measuring…</td></tr>;
  const { A, B } = measured;
  if (!A || !B) return <tr><td colSpan={5}>nothing to measure</td></tr>;
  const cell = (metric: Score['metrics'][number] | undefined) => metric
    ? <td class={metric.fit < 1 ? 'out' : undefined} title={`fit ${metric.fit.toFixed(3)}`}>{metric.value.toFixed(3)}</td>
    : <td />;
  const show = (value: number | null) => (value == null ? '—' : value.toFixed(3));
  return (
    <>
      {A.metrics.map((metric, index) => {
        const other = B.metrics[index];
        return (
          <tr key={metric.name}>
            <td>{metric.name}</td><td>{metric.low.toFixed(2)}–{metric.high.toFixed(2)}</td>
            {cell(metric)}{cell(other)}<Delta a={metric.value} b={other?.value} />
          </tr>
        );
      })}
      {(['interest', 'legibility', 'overall'] as const).map((key) => (
        <tr key={key} class="total">
          <td>{key}</td><td />
          <td>{show(A[key])}</td><td>{show(B[key])}</td><Delta a={A[key]} b={B[key]} />
        </tr>
      ))}
    </>
  );
}

export function ViewsFolder() {
  return (
    <>
      <div class="desk-head"><h2>Under the hood</h2><span class="engrave">what the poem compiled to</span></div>
      <nav class="views-nav">
        {VIEWS.map(([name, id, label]) => (
          <button key={name} id={id} aria-selected={view.value === name} onClick={() => { view.value = name; }}>{label}</button>
        ))}
      </nav>
      <pre id="pattern" class={view.value === 'pattern' ? 'wrapped' : 'wrapped hidden'}>{patternView.value}</pre>
      <pre id="explain" class={view.value === 'explain' ? undefined : 'hidden'}>{explainView.value}</pre>
      <div id="score" class={view.value === 'score' ? undefined : 'hidden'}>
        <div class="bar">
          <label>Judged against
            <select id="profile" value={profile.value} onInput={(event) => { profile.value = event.currentTarget.value; void measure(poem.value); }}>
              <option value="machine">machine — Kraftwerk, Daft Punk, psytrance</option>
              <option value="jazz">jazz — a piano trio, a modal quartet</option>
              <option value="interest">interest — generic</option>
            </select>
          </label>
        </div>
        <table>
          <thead><tr><th>metric</th><th>band</th><th>A</th><th>B</th><th>B − A</th></tr></thead>
          <tbody id="scoreRows"><ScoreRows measured={scores.value} /></tbody>
        </table>
        <p>
          A value outside its band is marked; its fit falls with the distance. Interest is the
          geometric mean of the fits; legibility comes from recompiling the poem with each word
          swapped, and asks whether every change is heard where the word is. Overall is the two
          multiplied. The bands are reasoned guesses, not measurements of records — trust your ears
          over them.
        </p>
      </div>
    </>
  );
}
