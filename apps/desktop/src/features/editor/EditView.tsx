import { useEffect, useMemo } from "react";
import { CropIcon, DiagnosticsIcon, OpenIcon } from "../../components/icons";
import { MarkControls } from "../../components/MarkControls";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { MarksDto } from "../../ipc/generated/MarksDto";
import { formatAperture, formatFocal, formatShutter } from "../../lib/format";
import { hasCommandModifier, isTextEntry } from "../../lib/keyboard";
import { markChangeForKey } from "../library/marks";
import type { EditSavingDto } from "../../ipc/generated/EditSavingDto";
import type { SaveState } from "./autosave";
import { geometryEdited, isIdentity } from "./recipe";
import { AdjustmentPanel } from "./AdjustmentPanel";
import { CropOverlay, CropToolbar, GeometryControls, useCropTool } from "./CropTool";
import { ChromaticAberrationToggle } from "./LensControls";
import { Histogram } from "./Histogram";
import { PanelSection } from "./PanelSection";
import { StatsPanel } from "./StatsPanel";
import type { Editor } from "./useEditor";
import { useToneCurve } from "./useToneCurve";
import { Viewer } from "./Viewer";

interface Props {
  editor: Editor;
  /** Marks of the open photo when it came from the Library; null otherwise. */
  marks: MarksDto | null;
  onMark: (change: MarkChangeDto) => void;
  /** Opens the previous (-1) or next (+1) photo of the Library view. */
  onStep: (delta: number) => void;
  /** Where the open photo sits in the Library view ("3 of 40"); null if not there. */
  position: { index: number; total: number } | null;
  onOpenFile: () => void;
}

/** The Edit mode: photograph in the centre, adjustments on the right. */
export function EditView({ editor, marks, onMark, onStep, position, onOpenFile }: Props) {
  const { info, image, recipe, busy } = editor;
  const toneCurve = useToneCurve(recipe);
  const fullSize = useMemo(
    () => (image ? { width: image.fullWidth, height: image.fullHeight } : null),
    [image?.fullWidth, image?.fullHeight],
  );
  const crop = useCropTool({
    recipe,
    imageId: image?.id ?? null,
    size: fullSize,
    onChange: editor.setRecipe,
    setViewTransform: editor.setViewTransform,
  });

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
            {image && recipe && (
              <EditStatus
                saving={image.editSaving}
                state={editor.saveState}
                edited={!isIdentity(recipe)}
                onReset={editor.resetRecipe}
              />
            )}
            {position && (
              <span className="photo-counter">
                {position.index + 1} of {position.total.toLocaleString()}
              </span>
            )}
            <button className="ghost" onClick={onOpenFile} disabled={busy || !info}>
              <OpenIcon />
              {busy ? "Opening…" : "Open photo…"}
            </button>
          </div>
        </div>
        <Viewer
          displayed={editor.displayed}
          loading={busy}
          onResize={editor.setTargetLongEdge}
          placeholder="Open a photo from the Library, or use “Open photo…”."
          overlay={crop.open ? <CropOverlay tool={crop} /> : undefined}
        />
        {/* The design's floating toolbar under the photo (zoom, masks and compare join
            it as they are built); while cropping, the crop toolbar takes its place. */}
        <div className="photo-toolbar-strip">
          {crop.open && info ? (
            <CropToolbar tool={crop} straighten={info.straighten} />
          ) : (
            image && (
              <div className="photo-toolbar" role="toolbar" aria-label="Photo tools">
                <button className="tool-button" title="Crop & straighten" onClick={crop.enter}>
                  <CropIcon size={15} />
                  Crop
                </button>
                {marks && (
                  <>
                    <span className="toolbar-divider" />
                    <MarkControls marks={marks} onChange={onMark} />
                  </>
                )}
              </div>
            )
          )}
        </div>
      </div>
      <aside className="panel-right" aria-label="Adjustments">
        <Histogram
          histogram={image && editor.displayed?.imageId === image.id ? editor.displayed.frame.histogram : null}
          specs={info?.adjustments ?? []}
          recipe={recipe}
          onChange={editor.setRecipe}
          disabled={!image}
          details={exif}
          emptyDetails={image ? "No exposure details" : ""}
        />
        <div className="panel-scroll scroll">
          {info && recipe && (
            <AdjustmentPanel
              specs={info.adjustments}
              mixerSpec={info.mixer}
              toneCurve={toneCurve}
              recipe={recipe}
              onChange={editor.setRecipe}
              disabled={!image}
              temperatureScale={image?.temperatureScale ?? null}
            />
          )}
          {info && recipe && (
            <PanelSection title="Geometry" icon={<CropIcon />} edited={geometryEdited(recipe)} defaultOpen={false}>
              <GeometryControls
                tool={crop}
                straighten={info.straighten}
                perspective={info.perspective}
                lens={{
                  edited: recipe.chromaticAberration !== undefined,
                  content: (
                    <ChromaticAberrationToggle
                      recipe={recipe}
                      imageId={image?.id ?? null}
                      size={fullSize}
                      onChange={editor.setRecipe}
                      disabled={!image}
                    />
                  ),
                }}
                disabled={!image}
              />
            </PanelSection>
          )}
          <PanelSection title="Diagnostics" icon={<DiagnosticsIcon />} defaultOpen={false}>
            <StatsPanel editor={editor} />
          </PanelSection>
        </div>
      </aside>
    </div>
  );
}

/**
 * The design's "● Edited" marker, with the save state and a way back to the original.
 * Photos outside the library, or edited in a newer version, say that edits aren't saved.
 */
function EditStatus({
  saving,
  state,
  edited,
  onReset,
}: {
  saving: EditSavingDto;
  state: SaveState | null;
  edited: boolean;
  onReset: () => void;
}) {
  const note =
    saving === "notInLibrary"
      ? "Not saved: this photo isn’t in your library"
      : saving === "newerVersion"
        ? "Edited in a newer version; changes here aren’t saved"
        : state === "saving"
          ? "Saving…"
          : state === "failed"
            ? "Couldn’t save"
            : null;
  return (
    <span className="edit-status" aria-live="polite">
      {edited && (
        <span className="edited-label">
          <span className="edited-dot" />
          Edited
        </span>
      )}
      {note && <span className={state === "failed" ? "edit-note failed" : "edit-note"}>{note}</span>}
      {edited && (
        <button className="ghost small" onClick={onReset} title="Back to the original look (the file itself was never changed)">
          Reset
        </button>
      )}
    </span>
  );
}
