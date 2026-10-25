// The page: the top bar and the notices, then the stage — the poem, the roll under it, and the
// folders beside it on a wide screen or under it on a phone — and the dialogs.
import { Download } from './dialogs/Download';
import { Loader } from './dialogs/Loader';
import { Welcome } from './dialogs/Welcome';
import { Editor } from './Editor';
import { Folders } from './Folders';
import { Notices } from './Notices';
import { Roll } from './Roll';
import { TopBar } from './TopBar';

export function App() {
  return (
    <>
      <TopBar />
      <Notices />
      <div class="stage">
        <Editor />
        <Roll />
        <Folders />
      </div>
      <Welcome />
      <Download />
      <Loader />
    </>
  );
}
