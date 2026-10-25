// Under the bar: what this is, until dismissed, and the status line — what the music is doing, and
// after it what the reading is doing.
import { dismissNotice } from '../actions';
import { noticeShown, statusLine } from '../store';

export function Notices() {
  const line = statusLine.value;
  return (
    <div class="notices">
      <div id="notice" hidden={!noticeShown.value}>
        <p><strong>An experiment:</strong> every word of the poem becomes part of the music. Edit a line
          and hear what changes.</p>
        <button id="noticeClose" type="button" aria-label="Dismiss" onClick={dismissNotice}>✕</button>
      </div>
      <div id="status" role="status" class={line.bad ? 'bad' : undefined}>{line.text}</div>
    </div>
  );
}
