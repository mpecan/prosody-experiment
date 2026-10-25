// While something big loads: the instruments on the first Play, a voice once chosen.
import { cancelLoader, hideLoader } from '../../reading';
import { loader } from '../../store';
import { Dialog } from './Dialog';

export function Loader() {
  const state = loader.value;
  const known = state?.fraction != null;
  return (
    <Dialog id="loader" labelledBy="loaderTitle" open={state !== null} onClose={hideLoader} stays>
      <div class="card">
        <h2 id="loaderTitle">{state?.title}</h2>
        <p id="loaderText" class="small">{state?.text}</p>
        <div class="bar-outer">
          <div id="loaderBar" class={known ? 'bar-inner' : 'bar-inner unknown'}
            style={known ? { width: Math.round((state?.fraction ?? 0) * 100) + '%' } : undefined} />
        </div>
        <p id="loaderProgress" class="small">{state?.progress}</p>
        <div class="actions">
          <button type="button" id="loaderHide" onClick={hideLoader}>{state?.hide}</button>
          <button type="button" id="loaderCancel" hidden={!state?.cancellable} onClick={cancelLoader}>Cancel</button>
        </div>
      </div>
    </Dialog>
  );
}
