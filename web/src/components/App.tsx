// The page: the top bar, the notices, the poem, the folders, and the dialogs.
import { Download } from './dialogs/Download';
import { Loader } from './dialogs/Loader';
import { Welcome } from './dialogs/Welcome';
import { Editor } from './Editor';
import { Folders } from './Folders';
import { Notices } from './Notices';
import { TopBar } from './TopBar';

export function App() {
  return (
    <>
      <TopBar />
      <Notices />
      <Editor />
      <Folders />
      <Welcome />
      <Download />
      <Loader />
    </>
  );
}
