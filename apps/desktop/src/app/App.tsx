import { watermarkFor } from "../features/export/watermark";
import { useCallback, useEffect, useRef, useState } from "react";
import { ErrorBanner } from "../components/ErrorBanner";
import { Toast, useToast } from "../components/Toast";
import { ExportDialog } from "../features/export/ExportDialog";
import { useExportQueue } from "../features/export/useExportQueue";
import type { ExportItemDto } from "../ipc/generated/ExportItemDto";
import { BrandMark, CloseIcon, ExportIcon, SettingsIcon } from "../components/icons";
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
  const library = useLibrary({
    view: settings.settings?.library.view ?? null,
    save: (view) => settings.update((s) => ({ ...s, library: { ...s.library, view } })),
  });
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

  // Saved edits show up in the Library (edited dot, new thumbnail when it next loads).
  const { lastSaved } = editor;
  const { markEdited } = library;
  useEffect(() => {
    if (lastSaved) markEdited(lastSaved.path, lastSaved.edited);
  }, [lastSaved, markEdited]);

  const exporting =
    editor.exportState !== null && (editor.exportState.last === null || editor.exportState.last.type === "progress");
  const context =
    mode === "settings" ? "Settings" : mode === "edit" ? (editor.image?.fileName ?? "") : (library.listing?.name ?? "");

  const { toast, notify } = useToast();

  // Exporting (ADR 0050): the open photo, with its edit as it is now, and the photos
  // ticked in the filmstrip, with their saved edits, through the export queue.
  const exports = useExportQueue(editor.reportError, notify);
  const [exportOpen, setExportOpen] = useState(false);
  const exportPaths = library.batch.filter((p) => p !== editPath);
  const exportCount = (editor.image ? 1 : 0) + exportPaths.length;
  const exportNames = (() => {
    const first = editor.image?.fileName ?? exportPaths[0]?.split(/[\\/]/).pop() ?? "";
    const others = exportCount - 1;
    return others > 0 ? `${first} and ${others} other${others === 1 ? "" : "s"}` : first;
  })();
  const runExport = async () => {
    let folder = settings.settings?.export.folder ?? null;
    if (!folder) {
      folder = await ipc.chooseExportFolder().catch((e: unknown) => {
        editor.reportError(e);
        return null;
      });
      await settings.reload();
      if (!folder) return;
    }
    const s = settings.settings?.export;
    const items: ExportItemDto[] = [
      ...(editor.image && editor.recipe ? [{ imageId: editor.image.id, recipe: editor.recipe }] : []),
      ...exportPaths.map((path) => ({ path })),
    ];
    if (items.length === 0) return;
    setExportOpen(false);
    const watermark = s ? await watermarkFor(s.watermark).catch(() => null) : null;
    await exports.start({
      items,
      longEdge: s?.longEdge ?? undefined,
      quality: s?.jpegQuality ?? 85,
      format: s?.format ?? "jpeg",
      sharpen: s?.sharpen ?? "screen",
      colourSpace: s?.colourSpace ?? "srgb",
      keepMetadata: s?.keepMetadata ?? true,
      stripLocation: s?.stripLocation ?? false,
      watermark: watermark ?? undefined,
    });
  };

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
          {exports.progress ? (
            <span className="export-progress" role="status">
              <span className="export-progress-text">
                Exporting {Math.min(exports.progress.done + 1, exports.progress.total)} of {exports.progress.total}
              </span>
              <span className="export-progress-bar" aria-hidden="true">
                <span
                  style={{
                    width: `${((exports.progress.done + exports.progress.fraction) / Math.max(1, exports.progress.total)) * 100}%`,
                  }}
                />
              </span>
              <button className="export-progress-cancel" aria-label="Stop exporting" title="Stop exporting" onClick={exports.cancel}>
                <CloseIcon size={12} />
              </button>
            </span>
          ) : (
            <button
              className="primary"
              onClick={() => setExportOpen(true)}
              disabled={mode !== "edit" || !editor.image || exporting}
              title={editor.image ? "Export this photo, and any ticked in the filmstrip" : "Open a photo to export it"}
            >
              <ExportIcon />
              {exporting ? "Exporting…" : "Export"}
            </button>
          )}
        </div>
      </header>
      {error && <ErrorBanner error={error} onDismiss={clearError} />}
      <div className="mode-body">
        {mode === "library" && (
          <LibraryView
            library={library}
            settings={settings}
            onOpenPhoto={openFromLibrary}
            notify={notify}
          />
        )}
        {mode === "edit" && (
          <EditView
            editor={editor}
            marks={editEntry?.marks ?? null}
            onMark={(change) => editEntry && void library.setMarks([editEntry.path], change)}
            position={(() => {
              const index = library.visible.findIndex((p) => p.path === editPath);
              return index >= 0 ? { index, total: library.visible.length } : null;
            })()}
            onStep={(delta) => {
              const next = library.neighbour(editPath, delta);
              if (next) openFromLibrary(next.path);
            }}
            notify={notify}
            library={library}
            currentPath={editEntry ? editPath : null}
            onOpenPhoto={openFromLibrary}
            onExport={() => setExportOpen(true)}
            onOpenFile={() => {
              setEditPath(null);
              void editor.openDialog();
            }}
          />
        )}
        {mode === "settings" && <SettingsView api={settings} />}
      </div>
      <QuitDialog />
      {exportOpen && settings.settings && (
        <ExportDialog
          count={exportCount}
          names={exportNames}
          frame={editor.displayed?.frame ?? null}
          settings={settings.settings.export}
          onChange={(change) => settings.update((s) => ({ ...s, export: { ...s.export, ...change } }))}
          onChooseFolder={() =>
            void ipc
              .chooseExportFolder()
              .then(() => settings.reload())
              .catch(editor.reportError)
          }
          onExport={() => void runExport()}
          onClose={() => setExportOpen(false)}
          photo={editor.image && editor.recipe ? { imageId: editor.image.id, recipe: editor.recipe } : null}
        />
      )}
      <Toast toast={toast} />
    </div>
  );
}
