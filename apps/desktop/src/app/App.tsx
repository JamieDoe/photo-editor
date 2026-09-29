import { useCallback, useEffect, useRef, useState } from "react";
import { ErrorBanner } from "../components/ErrorBanner";
import { BrandMark, ExportIcon, SettingsIcon } from "../components/icons";
import { QuitDialog } from "../components/QuitDialog";
import { EditView } from "../features/editor/EditView";
import { useEditor } from "../features/editor/useEditor";
import { LibraryView } from "../features/library/LibraryView";
import { useLibrary } from "../features/library/useLibrary";
import { SettingsView } from "../features/settings/SettingsView";
import { useSettings } from "../features/settings/useSettings";
import { runSelfTest } from "../features/selftest/runSelfTest";
import * as ipc from "../ipc/client";
import { WORKSPACES, type Mode } from "./modes";

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
  // Closing Settings returns to the workspace it was opened from.
  const workspaceRef = useRef<Mode>("library");
  if (mode !== "settings") workspaceRef.current = mode;

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

  // The Library photo open in Edit (null for files opened with "Open photo…"): its
  // marks are shown and set in Edit, and ← → step through the Library's photos.
  const [editPath, setEditPath] = useState<string | null>(null);
  const openFromLibrary = useCallback(
    (path: string) => {
      setEditPath(path);
      library.setSelected(path);
      setMode("edit");
      void editor.openPath(path);
    },
    [editor, library],
  );
  const editEntry = library.findPhoto(editPath);

  const exporting =
    editor.exportState !== null && (editor.exportState.last === null || editor.exportState.last.type === "progress");
  const context =
    mode === "settings" ? "Settings" : mode === "edit" ? (editor.image?.fileName ?? "") : (library.listing?.name ?? "");

  // One banner; the most relevant source first.
  const sources = [editor, library, settings];
  const source = sources.find((s) => s.error !== null);
  const error = source?.error ?? null;
  const clearError = () => source?.clearError();

  return (
    <div className="app">
      <header className="topbar">
        <div className="topbar-left">
          <BrandMark />
          {/* Working name until the product is named. */}
          <span className="brand">Photo Editor</span>
          {context && (
            <>
              <span className="topbar-divider" />
              <span className="topbar-context" title={context}>
                {context}
              </span>
            </>
          )}
        </div>
        <nav className="segmented" aria-label="Workspace">
          {WORKSPACES.map((m) => (
            <button key={m.id} aria-current={m.id === mode ? "page" : undefined} onClick={() => setMode(m.id)}>
              {m.label}
            </button>
          ))}
        </nav>
        <div className="topbar-right">
          <button
            className="icon-button"
            aria-label="Settings"
            title="Settings"
            aria-pressed={mode === "settings"}
            onClick={() => setMode(mode === "settings" ? workspaceRef.current : "settings")}
          >
            <SettingsIcon />
          </button>
          <button
            className="primary"
            onClick={() => void editor.exportImage()}
            disabled={mode !== "edit" || !editor.image || exporting}
            title={editor.image ? "Export this photo as a JPEG" : "Open a photo to export it"}
          >
            <ExportIcon />
            {exporting ? "Exporting…" : "Export"}
          </button>
        </div>
      </header>
      {error && <ErrorBanner error={error} onDismiss={clearError} />}
      <div className="mode-body">
        {mode === "library" && (
          <LibraryView
            library={library}
            settings={settings}
            onOpenPhoto={openFromLibrary}
          />
        )}
        {mode === "edit" && (
          <EditView
            editor={editor}
            marks={editEntry?.marks ?? null}
            onMark={(change) => editEntry && void library.setMarks([editEntry.path], change)}
            onStep={(delta) => {
              const next = library.neighbour(editPath, delta);
              if (next) openFromLibrary(next.path);
            }}
            onOpenFile={() => {
              setEditPath(null);
              void editor.openDialog();
            }}
          />
        )}
        {mode === "settings" && <SettingsView api={settings} />}
      </div>
      <QuitDialog />
    </div>
  );
}
