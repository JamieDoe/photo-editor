import { useEffect, useRef } from "react";
import * as ipc from "./ipc/client";
import { AdjustmentPanel } from "./features/editor/AdjustmentPanel";
import { StatsPanel } from "./features/editor/StatsPanel";
import { useEditor } from "./features/editor/useEditor";
import { Viewer } from "./features/editor/Viewer";
import { runSelfTest } from "./features/selftest/runSelfTest";

let selfTestStarted = false;

export function App() {
  const editor = useEditor();
  const editorRef = useRef(editor);
  editorRef.current = editor;

  useEffect(() => {
    if (selfTestStarted) return;
    selfTestStarted = true;
    void ipc.selfTestConfig().then(async (config) => {
      if (!config) return;
      const report = await runSelfTest(config, { editor: () => editorRef.current });
      await ipc.selfTestReport(report);
    });
  }, []);

  const { info, image, recipe, busy, error } = editor;
  const exportRunning = editor.exportState?.last?.type === "progress" || (editor.exportState !== null && editor.exportState.last === null);

  return (
    <div className="app">
      <header className="toolbar">
        <strong>Phase 0 prototype</strong>
        <button onClick={() => void editor.openDialog()} disabled={busy || !info}>
          {busy ? "Opening…" : "Open photo…"}
        </button>
        <button onClick={() => void editor.exportImage()} disabled={!image || exportRunning}>
          Export JPEG…
        </button>
        {error && (
          <span className="error" role="alert" onClick={editor.clearError}>
            {error}
          </span>
        )}
      </header>
      <main className="workspace">
        <Viewer
          displayed={editor.displayed}
          onResize={editor.setTargetLongEdge}
          placeholder={busy ? "Decoding…" : "Open a RAW or JPEG photo to begin."}
        />
        <aside className="sidebar">
          {info && recipe && (
            <AdjustmentPanel specs={info.adjustments} recipe={recipe} onChange={editor.setRecipe} disabled={!image} />
          )}
          <StatsPanel editor={editor} />
        </aside>
      </main>
    </div>
  );
}
