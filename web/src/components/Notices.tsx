// Under the bar: what this is, until dismissed, and the status line — what the music is doing, and
// after it what the reading is doing.
import { dismissNotice } from '../actions';
import { noticeShown, sharedOpened, statusLine } from '../store';

export function Notices() {
  const line = statusLine.value;
  return (
    <div class="notices">
      <div id="notice" hidden={!noticeShown.value}>
        <p><strong>An experiment:</strong> every word of the poem becomes part of the music. Edit a line
          and hear what changes.</p>
        <button id="noticeClose" type="button" aria-label="Dismiss" onClick={dismissNotice}>✕</button>
      </div>
      {sharedOpened.value && (
        <div id="sharedNote" class="shared-note">
          <p><strong>A shared poem.</strong> Someone made this; press Play to hear it. Change anything you
            like — your changes stay in this browser, and Share makes a link of your own.</p>
          <button type="button" aria-label="Dismiss" onClick={() => { sharedOpened.value = false; }}>✕</button>
        </div>
      )}
      <div id="status" role="status" class={line.bad ? 'bad' : undefined}>{line.text}</div>
    </div>
  );
}
