// The loader: for the instruments on the first Play, and for a voice once chosen. What is loading,
// how far it has got (a bar when the size is known, a sweep when not), Hide to let it carry on
// behind the page, and — for a voice — Cancel, which stops the download and keeps the voice there
// was.
import { loader, type EngineName } from './store';

/** While the loader is open: the engine it shows (null for the instruments), and the engine Cancel
 * goes back to. */
let loading: { engine: EngineName | null; back: EngineName | null } | null = null;

export const HIDE = 'Hide — keep loading';

export function openLoader({ title, text, engine = null, back = null }:
  { title: string; text: string; engine?: EngineName | null; back?: EngineName | null }): void {
  loading = { engine, back };
  loader.value = { title, text, progress: '', fraction: null, failed: false, cancellable: engine !== null, hide: HIDE };
}

export function closeLoader(): void {
  loading = null;
  loader.value = null;
}

/** The engine the open loader shows: undefined when it is closed, null for the instruments. */
export const loaderEngine = (): EngineName | null | undefined => (loading ? loading.engine : undefined);
/** The engine Cancel goes back to. */
export const loaderBack = (): EngineName | null => loading?.back ?? null;

export function loaderProgress(name: EngineName, progress: { text: string; fraction?: number | null }): void {
  if (loading?.engine !== name || !loader.value) return;
  loader.value = { ...loader.value, progress: progress.text, fraction: progress.fraction ?? null };
}

/** Says what the loader is doing now, for the engine it shows. */
export function loaderSays(name: EngineName, text: string): void {
  if (loading?.engine !== name || !loader.value) return;
  loader.value = { ...loader.value, progress: text };
}

export function loaderFailed(name: EngineName, label: string, error: Error): void {
  if (loading?.engine !== name || !loader.value) return;
  loader.value = {
    ...loader.value,
    title: `${label} could not load`,
    progress: `${error.message}. The browser's voice reads instead; choose it again to try again.`,
    fraction: 0,
    failed: true,
    cancellable: false,
    hide: 'Close',
  };
}
