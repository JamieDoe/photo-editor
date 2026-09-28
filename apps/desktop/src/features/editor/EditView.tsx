import { formatExposure } from "../../lib/format";
import { AdjustmentPanel } from "./AdjustmentPanel";
import { StatsPanel } from "./StatsPanel";
import type { Editor } from "./useEditor";
import { Viewer } from "./Viewer";

/** The Edit mode: photograph in the centre, adjustments on the right. */
export function EditView({ editor }: { editor: Editor }) {
  const { info, image, recipe, busy } = editor;
  const exporting =
    editor.exportState !== null && (editor.exportState.last === null || editor.exportState.last.type === "progress");
  return (
    <div className="edit-view">
      <div className="mode-toolbar">
        <button onClick={() => void editor.openDialog()} disabled={busy || !info}>
          {busy ? "Opening…" : "Open photo…"}
        </button>
        <button onClick={() => void editor.exportImage()} disabled={!image || exporting}>
          {exporting ? "Exporting…" : "Export JPEG…"}
        </button>
        {image && (
          <span className="photo-meta">
            <span>{image.fileName}</span>
            {image.camera && <span className="muted">{image.camera}</span>}
            {formatExposure(image) && <span className="mono muted">{formatExposure(image)}</span>}
          </span>
        )}
      </div>
      <main className="workspace">
        <Viewer
          displayed={editor.displayed}
          onResize={editor.setTargetLongEdge}
          placeholder={busy ? "Opening…" : "Open a photo from the Library, or use “Open photo…”."}
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
