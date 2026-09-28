import { useEffect, useRef, useState } from "react";
import { ErrorBanner } from "../components/ErrorBanner";
import { QuitDialog } from "../components/QuitDialog";
import { EditView } from "../features/editor/EditView";
import { useEditor } from "../features/editor/useEditor";
import { LibraryView } from "../features/library/LibraryView";
import { useLibrary } from "../features/library/useLibrary";
import { SettingsView } from "../features/settings/SettingsView";
import { useSettings } from "../features/settings/useSettings";
import { runSelfTest } from "../features/selftest/runSelfTest";
import * as ipc from "../ipc/client";
import { MODES, type Mode } from "./modes";

let selfTestStarted = false;

/**
 * Application shell: mode navigation around views that stay mounted state-wise.
 * Editor and settings state live here, so switching modes never loses the open photo
 * or its edits.
 */
export function App() {
  const editor = useEditor();
  const settings = useSettings();
  const library = useLibrary();
  const [mode, setMode] = useState<Mode>("library");
  const editorRef = useRef(editor);
  editorRef.current = editor;

  useEffect(() => {
    if (selfTestStarted) return;
    selfTestStarted = true;
    void ipc.selfTestConfig().then(async (config) => {
      if (!config) return;
      setMode("edit");
      const report = await runSelfTest(config, { editor: () => editorRef.current });
      await ipc.selfTestReport(report);
    });
  }, []);

  const openInEditor = async (open: () => Promise<unknown>) => {
    setMode("edit");
    await open();
  };

  // One banner; the most relevant source first.
  const sources = [editor, library, settings];
  const source = sources.find((s) => s.error !== null);
  const error = source?.error ?? null;
  const clearError = () => source?.clearError();

  return (
    <div className="app">
      <header className="toolbar">
        <nav className="modes" aria-label="Modes">
          {MODES.map((m) => (
            <button
              key={m.id}
              className={m.id === mode ? "active" : undefined}
              aria-current={m.id === mode ? "page" : undefined}
              onClick={() => setMode(m.id)}
            >
              {m.label}
            </button>
          ))}
        </nav>
      </header>
      {error && <ErrorBanner error={error} onDismiss={clearError} />}
      <div className="mode-body">
        {mode === "library" && (
          <LibraryView
            library={library}
            settings={settings}
            onOpenPhoto={(path) => void openInEditor(() => editor.openPath(path))}
          />
        )}
        {mode === "edit" && <EditView editor={editor} />}
        {mode === "settings" && <SettingsView api={settings} />}
      </div>
      <QuitDialog />
    </div>
  );
}
