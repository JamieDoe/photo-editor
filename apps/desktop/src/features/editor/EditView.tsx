import { useEffect } from "react";
import { DiagnosticsIcon, OpenIcon } from "../../components/icons";
import { MarkControls } from "../../components/MarkControls";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { MarksDto } from "../../ipc/generated/MarksDto";
import { formatAperture, formatFocal, formatShutter } from "../../lib/format";
import { hasCommandModifier, isTextEntry } from "../../lib/keyboard";
import { markChangeForKey } from "../library/marks";
import { AdjustmentPanel } from "./AdjustmentPanel";
import { PanelSection } from "./PanelSection";
import { StatsPanel } from "./StatsPanel";
import type { Editor } from "./useEditor";
import { Viewer } from "./Viewer";

interface Props {
  editor: Editor;
  /** Marks of the open photo when it came from the Library; null otherwise. */
  marks: MarksDto | null;
  onMark: (change: MarkChangeDto) => void;
  /** Opens the previous (-1) or next (+1) photo of the Library view. */
  onStep: (delta: number) => void;
  onOpenFile: () => void;
}

/** The Edit mode: photograph in the centre, adjustments on the right. */
export function EditView({ editor, marks, onMark, onStep, onOpenFile }: Props) {
  const { info, image, recipe, busy } = editor;

  // Keyboard: 0–5 / P / X / U mark the photo, ← → move through the Library's photos.
  // Ignored while a control (such as a slider) has focus, so its own keys still work.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (hasCommandModifier(e) || isTextEntry(e.target) || e.target instanceof HTMLInputElement) return;
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        onStep(e.key === "ArrowLeft" ? -1 : 1);
        e.preventDefault();
        return;
      }
      const change = marks ? markChangeForKey(e.key) : null;
      if (change) {
        onMark(change);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [marks, onMark, onStep]);
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
          <div className="meta-actions">
            {marks && <MarkControls marks={marks} onChange={onMark} />}
            <button className="ghost" onClick={onOpenFile} disabled={busy || !info}>
              <OpenIcon />
              {busy ? "Opening…" : "Open photo…"}
            </button>
          </div>
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
