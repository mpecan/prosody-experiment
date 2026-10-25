// Asked before a neural voice is downloaded for the first time. Closing it any other way than
// "Download" keeps the voice there was.
import { downloadQuestion } from '../../store';
import { Dialog } from './Dialog';

export function Download() {
  const question = downloadQuestion.value;
  const answer = (yes: boolean) => question?.answer(yes);
  return (
    <Dialog id="download" labelledBy="downloadTitle" open={question !== null} onClose={() => answer(false)}>
      <div class="card">
        <button class="close" type="button" aria-label="Close" onClick={() => answer(false)}>✕</button>
        <h2 id="downloadTitle">{question?.title}</h2>
        <p id="downloadText">{question?.text}</p>
        <div class="actions">
          <button class="primary" type="button" id="downloadYes" onClick={() => answer(true)}>{question?.yes}</button>
          <button type="button" id="downloadNo" onClick={() => answer(false)}>Keep the current voice</button>
        </div>
      </div>
    </Dialog>
  );
}
