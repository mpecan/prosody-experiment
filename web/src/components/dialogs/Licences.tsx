// The Licences dialog: what the site and the apps are made of, by whom, under what terms — the
// notices their licences ask to travel with every copy. Opened from the welcome ("about"). The
// words, sounds, voices and fonts are written here; the software is every package the build ships,
// listed by scripts/notices.mjs into notices.json and fetched when the dialog first opens.
import { useEffect, useState } from 'preact/hooks';

import { NEURAL_VOICES, SYSTEM_VOICE } from '../../reading/engines';
import { licencesOpen } from '../../store';
import { Dialog } from './Dialog';

/** notices.json, as scripts/notices.mjs writes it. */
export interface Notices {
  own: { prosody: string; cmudict: string };
  packages: { kind: 'crate' | 'npm'; name: string; version: string; licence: string; url: string; texts: string[]; standard?: boolean }[];
  texts: Record<string, string>;
}

let fetched: Promise<Notices> | null = null;
/** notices.json, fetched once. */
export function loadNotices(): Promise<Notices> {
  fetched ??= fetch(new URL('notices.json', document.baseURI)).then((response) => {
    if (!response.ok) throw new Error(`${response.status} for notices.json`);
    return response.json() as Promise<Notices>;
  });
  fetched.catch(() => { fetched = null; });   // asked again on the next open
  return fetched;
}

const close = () => { licencesOpen.value = false; };

/** A licence text, shown only once its row is opened: there are hundreds, many of them long. */
function Texts({ label, ids, notices, note }: { label: string; ids: string[]; notices: Notices; note?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <details onToggle={(event) => setOpen((event.currentTarget as HTMLDetailsElement).open)}>
      <summary>{label}</summary>
      {open && note && <p class="small">{note}</p>}
      {open && ids.map((id) => <pre key={id}>{notices.texts[id]}</pre>)}
    </details>
  );
}

export function Licences() {
  const open = licencesOpen.value;
  const [notices, setNotices] = useState<Notices | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    if (!open || notices) return;
    loadNotices().then(setNotices, (error: Error) => setFailed(error.message));
  }, [open, notices]);
  const packages = notices?.packages ?? [];
  return (
    <Dialog id="licences" labelledBy="licencesTitle" open={open} onClose={close}>
      <div class="card licences">
        <button class="close" type="button" aria-label="Close" onClick={close}>✕</button>
        <h2 id="licencesTitle">Licences and notices</h2>

        <h3>Prosody</h3>
        <p>© 2026 Matjaz Domen Pecan, under the MIT licence. The source:{' '}
          <a href="https://github.com/mpecan/prosody-experiment" rel="noopener" target="_blank">github.com/mpecan/prosody-experiment</a>.</p>
        {notices && <Texts label="The MIT licence" ids={[notices.own.prosody]} notices={notices} />}

        <h3>Words</h3>
        <p>Pronunciations come from the CMU Pronouncing Dictionary, © 1993–2015 Carnegie Mellon
          University, under a BSD licence.</p>
        {notices && <Texts label="The CMU dictionary's licence" ids={[notices.own.cmudict]} notices={notices} />}

        <h3>Sounds</h3>
        <p>The instruments are recordings from FluidR3 GM, © Frank Wen
          (<a href="soundfonts/LICENSES/FluidR3_GM.txt" target="_blank">MIT licence</a>), and GeneralUser
          GS by S. Christian Collins (<a href="soundfonts/LICENSES/GeneralUser_GS.txt" target="_blank">its
          licence</a>), by way of the webaudiofont files, © Srgy Surkv
          (<a href="soundfonts/LICENSES/webaudiofontdata.txt" target="_blank">MIT licence</a>).</p>
        <p>The drums are in the public domain (CC0 and the Unlicense): Switch Angel's recordings in
          uzu-drumkit; Terry93D's FM drum kit, cabled_mess's modular drums and friedensreich's modular
          kicks, on Freesound; Karoryfer Samples' Big Rusty Drums; and Virtuosity Drums.{' '}
          <a href="drums/SOURCES.md" target="_blank">Every file's source</a>.</p>

        <h3>Voices</h3>
        <p>The recorded readings of the example poems were spoken by Kokoro-82M (Apache-2.0) and by
          Piper voices: Cori, trained on public-domain LibriVox readings, and, in Slovenian, Artur,
          trained on the ARTUR speech corpus (CC BY 4.0) by ppisljar.</p>
        {SYSTEM_VOICE && <p>The system voice is your phone's own, under its terms.</p>}
        {NEURAL_VOICES && <p>The neural voices are fetched only when you choose one, and run in this
          page: Kokoro (kokoro-js and Transformers.js, Apache-2.0; the Kokoro-82M model, Apache-2.0),
          Piper (onnxruntime-web and piper-tts-web, MIT; each voice under its own licence) and Kitten
          (kitten-tts-webgpu, MIT; Kitten TTS nano, Apache-2.0). Each turns words into sounds with
          eSpeak NG, under the GPL-3.0:{' '}
          <a href="https://github.com/espeak-ng/espeak-ng" rel="noopener" target="_blank">its source</a>.</p>}

        <h3>Fonts</h3>
        <p>Spectral, © the Spectral Project Authors, and IBM Plex Mono, © IBM Corp., under the SIL Open
          Font License 1.1 (their texts are in the list below).</p>

        <h3>Software</h3>
        {failed && <p class="small" id="licences-failed">The package list could not be loaded ({failed}).</p>}
        {!notices && !failed && <p class="small">Loading the package list…</p>}
        {notices && <>
          <p>Prosody is built with {packages.length} open-source packages, each under its own licence.
            Open one for its licence texts.</p>
          <ul class="packages" id="packages">
            {packages.map((p) => (
              <li key={`${p.kind}:${p.name}@${p.version}`}>
                <Texts label={`${p.name} ${p.version} — ${p.licence}`} ids={p.texts} notices={notices}
                  note={p.standard ? 'Its package carries no licence file; its licence\'s standard text is given here.' : undefined} />
              </li>
            ))}
          </ul>
        </>}
      </div>
    </Dialog>
  );
}
