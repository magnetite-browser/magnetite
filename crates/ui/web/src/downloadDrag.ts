// A finished download dragged out of a list as its file: onto a web page
// (to upload it), into another app or a folder.
//
// macOS: the app starts the drag itself once the row's drag begins
// (`drag_download`), as a page cannot put a file into a drag there. Elsewhere
// a page drags any file it was given: on the press the app offers the file
// (`offer_download`), a hidden file chooser takes it without a dialog, and
// the row's drag then carries it. The page never learns the file's path.

import type { DOMAttributes } from "react";
import { IS_MAC, send, type DownloadEntry } from "./bridge";

/** The made-up type the hidden chooser asks for; the app answers only it (`DRAG_PICKER` in the engine). */
const PICKER_TYPE = ".magnetite-drag";

/** How long the chooser may take before the next press tries again. */
const TAKE_FOR_MS = 3000;

/** Files this page was given, by download id. */
const given = new Map<string, File>();
/** Whether a file is being taken: one at a time, so each chooser gets its own. */
let taking = false;

/** Take download `id`'s file into this page, if it is not here yet. */
function take(id: string) {
  if (taking || given.has(id)) return;
  taking = true;
  send({ cmd: "offer_download", id })
    .then(
      () =>
        new Promise<File | null>((resolve) => {
          const input = document.createElement("input");
          input.type = "file";
          input.accept = PICKER_TYPE;
          input.onchange = () => resolve(input.files?.[0] ?? null);
          input.oncancel = () => resolve(null);
          window.setTimeout(() => resolve(null), TAKE_FOR_MS);
          // Within the press's user activation, which a chooser needs.
          input.click();
        }),
    )
    .then((file) => {
      if (file) given.set(id, file);
    })
    .catch(() => {})
    .finally(() => {
      taking = false;
    });
}

/** What makes a row drag download `d`'s file: only a finished one that is still there. */
export function dragProps(d: DownloadEntry): DOMAttributes<HTMLElement> & { draggable?: boolean } {
  if (d.status !== "finished" || d.missing) return {};
  if (IS_MAC) {
    return {
      draggable: true,
      onDragStart: (e) => {
        e.preventDefault();
        send({ cmd: "drag_download", id: d.id }).catch(() => {});
      },
    };
  }
  return {
    draggable: true,
    onPointerDown: (e) => {
      if (e.button === 0) take(d.id);
    },
    onDragStart: (e) => {
      const file = given.get(d.id);
      // Not taken yet (a very quick drag): nothing to carry.
      if (!file) return e.preventDefault();
      e.dataTransfer.items.add(file);
      e.dataTransfer.effectAllowed = "copy";
    },
  };
}
