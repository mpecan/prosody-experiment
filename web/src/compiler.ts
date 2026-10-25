// Where the page's questions go. Served by `prosody-web`, they are HTTP requests to it; built as
// static files (`scripts/build-site.sh`), the whole compiler is `prosody_bg.wasm` beside the page
// (bound by `prosody.js`) and answers the same requests in-process, so a static host needs nothing
// else. Either way the answer is a `Response`, so nothing else knows which.
//
// The compiler runs in two workers, never on the page's own thread: compiling "Patterns" takes
// about a second there and scoring it seven, and the page's thread is the one Strudel's scheduler
// ticks on — a block there while the music plays is music skipped. One worker answers what plays
// (pattern, trace, reading, explanation), the other only measures, so a new pattern never waits
// behind the scores of the last one. Each loads its own copy of the compiler; their answers are
// the same bytes, which `scripts/check-wasm.sh` checks against the server.
import { site } from './data';

interface Answer {
  status: number;
  body: string;
}
type Thread = (target: string, body: string) => Promise<Answer>;
interface Bindings {
  default: () => Promise<unknown>;
  respond: (target: string, body: string) => { status: number; body: string; free: () => void };
}

/** The address of the generated bindings, beside the page — never bundled: they are built apart. */
export const bindingsUrl = () => new URL('./prosody.js', document.baseURI).href;

// `initStrudel` fills the global scope with names of its own, `Response` not among them today; the
// page's own is kept anyway, so an answer is always a real one.
const NativeResponse = Response;

let compiler: Promise<Bindings> | null = null;
function loadCompiler(): Promise<Bindings> {
  // The generated module finds `prosody_bg.wasm` beside itself.
  compiler ??= (import(/* @vite-ignore */ bindingsUrl()) as Promise<Bindings>).then(async (bindings) => {
    await bindings.default();
    return bindings;
  });
  return compiler;
}

/** A worker's script, as text: a worker made from a blob resolves nothing relative to the page, so
 * it is handed the bindings' absolute address. */
export function compilerThreadSource(bindings: string): string {
  return `
  const bindings = import(${JSON.stringify(bindings)})
    .then(async (module) => { await module.default(); return module; });
  onmessage = async ({ data: { id, target, body } }) => {
    try {
      const { respond } = await bindings;
      const answer = respond(target, body);
      postMessage({ id, status: answer.status, body: answer.body });
      answer.free();
    } catch (error) {
      postMessage({ id, status: 500, body: 'the compiler failed: ' + error });
    }
  };`;
}

function compilerThread(): Thread {
  const source = compilerThreadSource(bindingsUrl());
  const worker = new Worker(URL.createObjectURL(new Blob([source], { type: 'text/javascript' })), { type: 'module' });
  const waiting = new Map<number, (answer: Answer) => void>();
  let asked = 0;
  worker.onmessage = ({ data }: MessageEvent<Answer & { id: number }>) => {
    waiting.get(data.id)?.(data);
    waiting.delete(data.id);
  };
  return (target, body) => new Promise((resolve) => {
    asked += 1;
    waiting.set(asked, resolve);
    worker.postMessage({ id: asked, target, body });
  });
}

let threads: { compile: Thread; measure: Thread } | null = null;
/** The worker that answers `path`: scores apart from everything else. */
export function threadFor(path: string): Thread {
  if (threads === null) {
    try {
      threads = { compile: compilerThread(), measure: compilerThread() };
    } catch (error) {
      // No module workers: the compiler answers in-process, as it did before them.
      console.warn('[compiler] no worker, compiling on the page:', error);
      const inProcess: Thread = async (target, body) => {
        const { respond } = await loadCompiler();
        const answer = respond(target, body);
        const reply = { status: answer.status, body: answer.body };
        answer.free();
        return reply;
      };
      threads = { compile: inProcess, measure: inProcess };
    }
  }
  return path === '/score' ? threads.measure : threads.compile;
}

/** Asks the compiler for one view of `body` under the settings in `query`. */
export async function ask(path: string, query: string, body: string): Promise<Response> {
  if (site.transport === 'server') return fetch(new URL('.' + path + '?' + query, document.baseURI), { method: 'POST', body });
  const answer = await threadFor(path)(path + '?' + query, body);
  return new NativeResponse(answer.body, { status: answer.status });
}
