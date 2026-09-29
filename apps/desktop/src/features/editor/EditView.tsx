import { DiagnosticsIcon, OpenIcon } from "../../components/icons";
import { formatAperture, formatFocal, formatShutter } from "../../lib/format";
import { AdjustmentPanel } from "./AdjustmentPanel";
import { PanelSection } from "./PanelSection";
import { StatsPanel } from "./StatsPanel";
import type { Editor } from "./useEditor";
import { Viewer } from "./Viewer";

/** The Edit mode: photograph in the centre, adjustments on the right. */
export function EditView({ editor }: { editor: Editor }) {
  const { info, image, recipe, busy } = editor;
  const exif = image
    ? [
        image.iso != null ? `ISO ${image.iso}` : null,
        formatFocal(image.focalLengthMm),
        formatAperture(image.aperture),
        formatShutter(image.shutterSeconds),
      ].filter((x): x is string => x !== null)
    : [];
  const size = image ? `${((image.fullWidth * image.fullHeight) / 1e6).toFixed(1)} MP` : "";
  return (
    <div className="edit-view">
      <div className="stage-column">
        <div className="meta-row">
          <div className="meta-title">
            {image ? (
              <>
                <span className="photo-name">{image.fileName}</span>
                <span className="mono">
                  {[image.camera, image.cameraRaw ? "RAW" : "JPEG", size].filter(Boolean).join(" · ")}
                </span>
              </>
            ) : (
              <span className="subtle">No photo open</span>
            )}
          </div>
          <button className="ghost" onClick={() => void editor.openDialog()} disabled={busy || !info}>
            <OpenIcon />
            {busy ? "Opening…" : "Open photo…"}
          </button>
        </div>
        <Viewer
          displayed={editor.displayed}
          onResize={editor.setTargetLongEdge}
          placeholder={busy ? "Opening…" : "Open a photo from the Library, or use “Open photo…”."}
        />
      </div>
      <aside className="panel-right" aria-label="Adjustments">
        <div className={exif.length > 0 ? "panel-exif" : "panel-exif empty"}>
          {exif.length > 0 ? exif.map((x) => <span key={x}>{x}</span>) : image ? "No exposure details" : ""}
        </div>
        <div className="panel-scroll scroll">
          {info && recipe && (
            <AdjustmentPanel specs={info.adjustments} recipe={recipe} onChange={editor.setRecipe} disabled={!image} />
          )}
          <PanelSection title="Diagnostics" icon={<DiagnosticsIcon />} defaultOpen={false}>
            <StatsPanel editor={editor} />
          </PanelSection>
        </div>
      </aside>
    </div>
  );
}
