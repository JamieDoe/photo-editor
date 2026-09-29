import { useEffect, useState } from "react";
import * as ipc from "../ipc/client";

/**
 * Confirms quitting while exports are running. Shown when Rust holds a window close
 * or quit (see src-tauri/src/lifecycle.rs).
 */
export function QuitDialog() {
  const [exportsRunning, setExportsRunning] = useState(0);
  const [quitting, setQuitting] = useState(false);

  useEffect(() => {
    const unlisten = ipc.onQuitRequested((e) => setExportsRunning(e.exportsRunning)).catch(() => () => {});
    return () => void unlisten.then((u) => u());
  }, []);

  if (exportsRunning === 0) return null;
  const plural = exportsRunning === 1 ? "An export is" : `${exportsRunning} exports are`;
  return (
    <div className="modal-backdrop">
      <div className="modal" role="alertdialog" aria-labelledby="quit-title" aria-describedby="quit-body">
        <h2 id="quit-title">{plural} still running</h2>
        <p id="quit-body">If you quit now, the export will be cancelled. No partial files are left behind.</p>
        <div className="modal-actions">
          <button autoFocus onClick={() => setExportsRunning(0)} disabled={quitting}>
            Keep working
          </button>
          <button
            className="danger"
            disabled={quitting}
            onClick={() => {
              setQuitting(true);
              void ipc.quit();
            }}
          >
            {quitting ? "Quitting…" : "Cancel export and quit"}
          </button>
        </div>
      </div>
    </div>
  );
}
