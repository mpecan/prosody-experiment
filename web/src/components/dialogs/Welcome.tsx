// The welcome: open on a first visit; once closed it stays closed, and "about" opens it again.
import { closeWelcome } from '../../actions';
import { licencesOpen, welcomeOpen } from '../../store';
import { Dialog } from './Dialog';

// The licences take the welcome's place: one card at a time.
function openLicences(): void {
  closeWelcome();
  licencesOpen.value = true;
}

export function Welcome() {
  return (
    <Dialog id="welcome" labelledBy="welcomeTitle" open={welcomeOpen.value} onClose={closeWelcome}>
      <div class="card">
        <button class="close" type="button" aria-label="Close" onClick={closeWelcome}>✕</button>
        <div class="entry">
          <div class="head" id="welcomeTitle"><span>pro</span><span>so</span><span>dy</span><span class="pron">/ˈprɒsədi/</span></div>
          <div class="pos">noun</div>
          <ol>
            <li>the patterns of rhythm and sound in poetry: where the stresses fall, how the lines move,
              what rhymes with what.</li>
            <li><span class="field">linguistics</span> the stress and intonation of speech.</li>
          </ol>
        </div>
        <p class="lead"><strong>This is an experiment.</strong> Here a poem is the source code for a piece
          of music.</p>
        <p>Each line becomes a bar. Stressed syllables become the beat. Rhymes become notes. Repeated lines
          become loops that build up and wear away. A voice can read the poem over it. Every sound
          traces back to a word, and the words light up as they play. Change a word and you'll hear
          exactly what it changed.</p>
        <p>Pick a poem, or write your own, and press Play.</p>
        <div class="actions"><button class="primary" type="button" onClick={closeWelcome}>Start listening</button></div>
        <p class="small">How it works, and the source: <a href="https://github.com/mpecan/prosody-experiment"
          rel="noopener" target="_blank">github.com/mpecan/prosody-experiment</a>. Questions, poems,
          ideas: <a href="mailto:me@prosody.rocks">me@prosody.rocks</a>.</p>
        {/* What the page is made of travels with it, and its notices with that (Licences.tsx). */}
        <p class="small" id="credits">The instruments are recordings from FluidR3 GM and GeneralUser GS;
          the drums are in the public domain; the pronunciations come from the CMU Pronouncing
          Dictionary. Who made what, and under what terms:{' '}
          <button type="button" class="link" id="open-licences" onClick={openLicences}>licences and notices</button>.</p>
      </div>
    </Dialog>
  );
}
