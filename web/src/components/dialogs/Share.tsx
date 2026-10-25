// The share dialog: a link that holds the poem and how it sounds — nothing is stored anywhere; the
// link is the creation (share.ts). Copy it, or hand it to the phone's own share sheet.
import { copyShare } from '../../actions';
import { shareLink } from '../../store';
import { Dialog } from './Dialog';

/** Past this many characters some chat apps shorten or break a link. */
const LONG = 2000;

export function Share() {
  const open = shareLink.value;
  const close = () => { shareLink.value = null; };
  const share = async () => {
    try {
      await navigator.share({ title: 'A poem on prosody.rocks', url: open?.link });
    } catch {
      // Dismissed, or not allowed: the link is still here to copy.
    }
  };
  return (
    <Dialog id="shareDialog" labelledBy="shareTitle" open={open !== null} onClose={close}>
      <div class="card">
        <button class="close" type="button" aria-label="Close" onClick={close}>✕</button>
        <h2 id="shareTitle">Share this poem</h2>
        <p>The link holds the poem and how it sounds — its style, its controls and its A and B. Nothing
          is stored anywhere: whoever opens it hears exactly this, made again in their browser.</p>
        <input id="shareLink" class="share-link" readOnly value={open?.link ?? ''} aria-label="The link"
          onFocus={(event) => event.currentTarget.select()} />
        {open && open.link.length > LONG && (
          <p class="small">A long poem makes a long link ({open.link.length.toLocaleString()} characters): it works, but
            some chat apps shorten long links, so a message or an email carries it best.</p>
        )}
        <div class="actions">
          <button class="primary" type="button" id="shareCopy" onClick={() => void copyShare()}>{open?.copied ? 'Copied' : 'Copy the link'}</button>
          {'share' in navigator && <button type="button" id="shareNative" onClick={() => void share()}>Share…</button>}
        </div>
      </div>
    </Dialog>
  );
}
