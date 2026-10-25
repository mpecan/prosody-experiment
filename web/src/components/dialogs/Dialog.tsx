// A modal card over the dimmed page. Open while `open`; closed by its own buttons, Escape, or —
// unless `stays` — a click on the dimmed page around it, each of which calls `onClose`.
import type { ComponentChildren } from 'preact';
import { useEffect, useRef } from 'preact/hooks';

export function Dialog({ id, open, onClose, labelledBy, stays = false, children }: {
  id: string;
  open: boolean;
  onClose: () => void;
  labelledBy: string;
  stays?: boolean;
  children: ComponentChildren;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  // Whether the page still wants it open: a close the page asked for is not the reader's.
  const wanted = useRef(open);
  wanted.current = open;
  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (open && !element.open) element.showModal();
    if (!open && element.open) element.close();
  }, [open]);
  return (
    <dialog id={id} ref={dialog} aria-labelledby={labelledBy}
      onClose={() => { if (wanted.current) onClose(); }}
      onClick={(event) => { if (!stays && event.target === dialog.current) dialog.current?.close(); }}>
      {children}
    </dialog>
  );
}
