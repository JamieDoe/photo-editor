import { useEffect, useMemo, useRef, useState } from "react";
import { CalibrationIcon, ColourIcon, CompareIcon, RetouchIcon, CropIcon, DiagnosticsIcon, MaskIcon, OpenIcon, RedoIcon, SearchIcon, UndoIcon } from "../../components/icons";
import { MarkControls } from "../../components/MarkControls";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { MarksDto } from "../../ipc/generated/MarksDto";
import { formatAperture, formatFocal, formatShutter } from "../../lib/format";
import { hasCommandModifier, isTextEntry } from "../../lib/keyboard";
import { markChangeForKey } from "../library/marks";
import type { LibraryApi } from "../library/useLibrary";
import * as ipc from "../../ipc/client";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { EditSavingDto } from "../../ipc/generated/EditSavingDto";
import type { ToneSetting } from "../../ipc/generated/ToneSetting";
import type { SaveState } from "./autosave";
import { geometryEdited, isIdentity } from "./recipe";
import { formatSliderValue } from "./sliderTrack";
import { AdjustmentPanel } from "./AdjustmentPanel";
import { CompareOverlay, useCompare } from "./Compare";
import { CropOverlay, CropToolbar, GeometryControls, useCropTool } from "./CropTool";
import { ChromaticAberrationToggle } from "./LensControls";
import { Histogram } from "./Histogram";
import { MaskOverlay, MaskToolbar, useMaskTool } from "./MaskTool";
import { RetouchControls, RetouchOverlay, useRetouchTool } from "./RetouchTool";
import { SelectiveControls } from "./SelectiveControls";
import { Filmstrip } from "./Filmstrip";
import { PanelFooter } from "./PanelFooter";
import { PanelSection } from "./PanelSection";
import { ColourGradingControls } from "./ColourGradingControls";
import { gradingEdited } from "./colourGrading";
import { CalibrationControls } from "./CalibrationControls";
import { calibrationEdited } from "./calibration";
import { PresetStrip } from "./PresetStrip";
import { StatsPanel } from "./StatsPanel";
import type { Editor } from "./useEditor";
import { Viewer } from "./Viewer";
import { useZoom } from "./useZoom";
import { zoomPercent } from "./zoom";

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
  /** Shows a short confirmation ("Edits copied"). */
  notify: (message: string) => void;
  /** The Library, for the filmstrip and batch editing (ADR 0049). */
  library: LibraryApi;
  /** The Library photo open here, if the open photo is one. */
  currentPath: string | null;
  onOpenPhoto: (path: string) => void;
  /** Opens the export dialog (the batch bar's Export…). */
  onExport: () => void;
  /** Focus mode (ADR 0072): the panels and filmstrip hidden; F toggles it, Esc ends it. */
  focus: boolean;
  onFocus: (on: boolean) => void;
}

/** The Edit mode: photograph in the centre, adjustments on the right. */
export function EditView({ editor, marks, onMark, onStep, position, onOpenFile, notify, library, currentPath, onOpenPhoto, onExport, focus, onFocus }: Props) {
  const { info, image, recipe, busy } = editor;
  // The histogram of what the viewer shows, for the panel's graph and the tone curve.
  const histogram = image && editor.displayed?.imageId === image.id ? editor.displayed.frame.histogram : null;
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
  const masks = useMaskTool({ recipe, imageId: image?.id ?? null, onChange: editor.setRecipe });
  const compare = useCompare();
  // The Retouch section open puts the photo in retouch mode (ADR 0054), unless
  // another tool takes it.
  const [retouchOpen, setRetouchOpen] = useState(false);
  const retouch = useRetouchTool({
    recipe,
    imageId: image?.id ?? null,
    aspect: image ? image.fullWidth / image.fullHeight : 1.5,
    active: retouchOpen && !crop.open && !masks.open && !compare.open,
    onChange: editor.setRecipe,
    notify,
  });
  // One at a time: cropping, masking, comparing or retouching.
  const enterCrop = () => {
    masks.done();
    compare.close();
    crop.enter();
  };
  const enterMasks = () => {
    crop.done();
    compare.close();
    masks.enter();
  };
  const pickMask = (id: number) => {
    crop.done();
    compare.close();
    masks.pick(id);
  };
  const toggleCompare = () => {
    crop.done();
    masks.done();
    compare.toggle();
  };
  const toggleRetouch = (open: boolean) => {
    if (open) {
      crop.done();
      masks.done();
      compare.close();
    }
    setRetouchOpen(open);
  };
  const retouching = retouchOpen && !crop.open && !masks.open && !compare.open;
  const frame = editor.displayed?.frame;

  // Zoom (ADR 0070): Fit, or from just above it to 800 %, also while retouching or
  // masking (their marks are drawn on the zoomed photo); cropping and comparing fit
  // it. It stays from photo to photo, once each one's first render has arrived.
  const zoomable = image !== null && !crop.open && !compare.open;
  const zooming = useZoom(zoomable);
  const zoom = editor.displayed?.imageId === image?.id ? zooming.zoom : null;
  const zoomModel = zooming.model;
  const toggleZoom = () => zoomModel.toggle();
  const zoomButton = (
    <button
      className="tool-button zoom-button"
      title={zoom ? "Fit the photo (Z, ⌘−)" : "Zoom to 100 % (Z, ⌘+)"}
      aria-label={zoom ? `Zoom: ${zoomPercent(zoom.scale)}, fit the photo` : "Zoom: fit, zoom to 100 %"}
      onClick={toggleZoom}
      disabled={!zoomable}
    >
      <SearchIcon size={15} />
      <span className="mono zoom-label">{zoom ? zoomPercent(zoom.scale) : "Fit"}</span>
    </button>
  );

  // Keyboard: 0–5 / P / X / U mark the photo, ← → move through the Library's photos.
  // Ignored while a control (such as a slider) has focus, so its own keys still work.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || hasCommandModifier(e) || isTextEntry(e.target) || e.target instanceof HTMLInputElement) return;
      if (e.target instanceof Element && e.target.closest('[role="slider"]')) return;
      // \ shows the photo before and after editing, as in other editors.
      if (e.key === "\\" && image) {
        toggleCompare();
        e.preventDefault();
        return;
      }
      // F toggles focus mode; Esc ends it when no tool takes Esc for itself.
      if (e.key.toLowerCase() === "f" && !e.altKey) {
        onFocus(!focus);
        e.preventDefault();
        return;
      }
      if (e.key === "Escape" && focus && !crop.open && !masks.open && !compare.open) {
        onFocus(false);
        e.preventDefault();
        return;
      }
      // Z toggles Fit and 100 %, as in Lightroom.
      if (e.key.toLowerCase() === "z" && !e.altKey && zoomable) {
        toggleZoom();
        e.preventDefault();
        return;
      }
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        onStep(e.key === "ArrowLeft" ? -1 : 1);
        e.preventDefault();
        return;
      }
      const change = marks ? markChangeForKey(e.key, marks) : null;
      if (change) {
        onMark(change);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [marks, onMark, onStep, image, toggleCompare, zoomable, zoomModel, focus, onFocus, crop.open, masks.open, compare.open]);

  // Copy and paste (ADR 0048), confirmed as the design does. With photos ticked in the
  // filmstrip, Paste also applies to them, and Sync edits gives them this photo's edit
  // (ADR 0049): they are written straight to the library.
  const [syncing, setSyncing] = useState(false);
  /** Sets `groups` from `source` on the ticked photos other than the open one. */
  const applyToTicked = async (source: EditRecipe, groups: string[]) => {
    const targets = library.batch.filter((p) => p !== currentPath);
    if (targets.length === 0) return { applied: 0, failed: 0 };
    const result = await ipc.pasteEditsTo(targets, source, groups);
    for (const a of result.applied) library.markEdited(a.path, a.edited);
    if (result.failed.length > 0) console.warn("batch edits left out", result.failed);
    return { applied: result.applied.length, failed: result.failed.length };
  };
  const photos = (n: number) => `${n} photo${n === 1 ? "" : "s"}`;
  const leftOut = (n: number) => (n > 0 ? ` · ${photos(n)} couldn’t be changed` : "");
  const copyEdits = () => {
    if (editor.copyEdits()) notify("Edits copied");
  };
  const pasteEdits = async () => {
    const copied = editor.copied;
    const done = editor.pasteEdits();
    if (!copied || library.batch.length === 0) {
      if (done === "pasted") notify("Pasted to 1 photo");
      else if (done === "same") notify("This photo already has these edits");
      return;
    }
    try {
      const others = await applyToTicked(copied.recipe, copied.groups);
      const onOpen = done === "nothing" ? 0 : 1;
      notify(`Pasted to ${photos(others.applied + onOpen)}${leftOut(others.failed)}`);
    } catch (e) {
      editor.reportError(e);
    }
  };
  const syncEdits = async () => {
    if (!recipe || syncing) return;
    setSyncing(true);
    try {
      const done = await applyToTicked(recipe, editor.copyGroups);
      notify(done.applied > 0 ? `Synced edits to ${photos(done.applied)}${leftOut(done.failed)}` : "Tick other photos to sync this one’s edits to");
    } catch (e) {
      editor.reportError(e);
    } finally {
      setSyncing(false);
    }
  };

  // Auto (ADR 0071): the tone sliders as a starting point, measured on the photo as
  // edited, applied as one step. The newest edit is the one it is applied to.
  const [autoBusy, setAutoBusy] = useState(false);
  const recipeRef = useRef(recipe);
  recipeRef.current = recipe;
  const runAuto = async () => {
    if (!image || !recipe || autoBusy) return;
    setAutoBusy(true);
    try {
      const tone = await ipc.autoTone(image.id, recipe);
      const latest = recipeRef.current;
      if (latest && editor.image?.id === image.id) {
        editor.setRecipe({ ...latest, ...tone }, "Auto tone");
        notify("Auto tone applied");
      }
    } catch (e) {
      if (!ipc.isCancellation(e)) editor.reportError(e);
    } finally {
      setAutoBusy(false);
    }
  };
  /** Auto for one setting (Shift-double-click on its slider), as one step. */
  const runAutoSetting = async (setting: ToneSetting) => {
    if (!image || !recipe) return;
    const spec = info?.adjustments.find((a) => a.key === setting);
    const label = spec?.label ?? setting;
    try {
      const value = await ipc.autoSetting(image.id, recipe, setting);
      const latest = recipeRef.current;
      if (latest && editor.image?.id === image.id) {
        // Said with its value, or that it was already right.
        if (latest[setting] === value) {
          notify(`${label} already suits this photo`);
          return;
        }
        editor.setRecipe({ ...latest, [setting]: value }, `Auto ${label}`);
        notify(spec ? `Auto ${label} ${formatSliderValue(value, spec.min, spec.step, spec.unit)}` : `Auto ${label}`);
      }
    } catch (e) {
      if (!ipc.isCancellation(e)) editor.reportError(e);
    }
  };
  const autoRef = useRef(runAuto);
  autoRef.current = runAuto;

  // ⌘Z undoes, ⇧⌘Z (or ⌘Y) redoes (ADR 0044); ⇧⌘U is Auto, as in Lightroom; ⇧⌘C and ⇧⌘V copy and paste edits, as in
  // Lightroom (⌘C and ⌘V stay with text); ⌘+ and ⌘− zoom in and out a level, ⌘0 fits
  // (ADR 0070). Text fields keep their own keys.
  const { undo, redo } = editor;
  const copyRef = useRef(copyEdits);
  const pasteRef = useRef(pasteEdits);
  copyRef.current = copyEdits;
  pasteRef.current = pasteEdits;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!hasCommandModifier(e) || isTextEntry(e.target)) return;
      const key = e.key.toLowerCase();
      if (key === "z") (e.shiftKey ? redo : undo)();
      else if (key === "y") redo();
      else if (key === "c" && e.shiftKey) copyRef.current();
      else if (key === "v" && e.shiftKey) void pasteRef.current();
      else if (key === "u" && e.shiftKey) void autoRef.current();
      else if ((key === "=" || key === "+") && zoomable) zoomModel.step(1);
      else if (key === "-" && zoomable) zoomModel.step(-1);
      else if (key === "0" && zoomable) zoomModel.fit();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo, zoomable, zoomModel]);
  const exif = image
    ? [
        image.iso != null ? `ISO ${image.iso}` : null,
        formatFocal(image.focalLengthMm),
        formatAperture(image.aperture),
        formatShutter(image.shutterSeconds),
      ].filter((x): x is string => x !== null)
    : [];
  const size = image ? `${((image.fullWidth * image.fullHeight) / 1e6).toFixed(1)} MP` : "";
  const details = image ? [image.camera, image.cameraRaw ? "RAW" : "JPEG", size].filter(Boolean).join(" · ") : "";
  return (
    <div className={focus ? "edit-view focus" : "edit-view"}>
      <div className="stage-column">
        <div className="meta-row">
          <div className="meta-title">
            {image ? (
              <>
                <span className="photo-name" title={image.fileName}>
                  {image.fileName}
                </span>
                <span className="mono" title={details}>
                  {details}
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
              />
            )}
            {image && (
              <span className="history-buttons">
                <button
                  className="icon-button history-button"
                  onClick={undo}
                  disabled={editor.undoLabel === null}
                  aria-label={editor.undoLabel ? `Undo ${editor.undoLabel}` : "Undo"}
                  title={editor.undoLabel ? `Undo ${editor.undoLabel} (⌘Z)` : "Nothing to undo"}
                >
                  <UndoIcon />
                </button>
                <button
                  className="icon-button history-button"
                  onClick={redo}
                  disabled={editor.redoLabel === null}
                  aria-label={editor.redoLabel ? `Redo ${editor.redoLabel}` : "Redo"}
                  title={editor.redoLabel ? `Redo ${editor.redoLabel} (⇧⌘Z)` : "Nothing to redo"}
                >
                  <RedoIcon />
                </button>
              </span>
            )}
            {position && (
              <span className="photo-counter">
                {position.index + 1} of {position.total.toLocaleString()}
              </span>
            )}
            {/* Icon-only on narrow stages (styles.css), so it keeps its name as a label. */}
            <button
              className="ghost open-photo"
              onClick={onOpenFile}
              disabled={busy || !info}
              aria-label={busy ? "Opening…" : "Open photo…"}
              title="Open photo…"
            >
              <OpenIcon />
              <span className="button-label">{busy ? "Opening…" : "Open photo…"}</span>
            </button>
          </div>
        </div>
        <Viewer
          displayed={editor.displayed}
          loading={busy}
          onResize={editor.setTargetLongEdge}
          zoom={zoom}
          zoomControl={zoomable ? zoomModel : undefined}
          windowed={editor.windowed}
          onWindow={editor.setViewWindow}
          placeholder="Open a photo from the Library, or use “Open photo…”."
          overlay={
            crop.open ? (
              <CropOverlay tool={crop} />
            ) : masks.open && frame ? (
              <MaskOverlay tool={masks} size={{ width: frame.fullWidth, height: frame.fullHeight }} />
            ) : compare.open && image && recipe && info ? (
              <CompareOverlay compare={compare} editor={editor} recipe={recipe} specs={info.adjustments} />
            ) : retouching && frame && image && recipe ? (
              <RetouchOverlay
                tool={retouch}
                size={{ width: frame.fullWidth, height: frame.fullHeight }}
                recipe={recipe}
                photo={{ width: image.fullWidth, height: image.fullHeight }}
              />
            ) : undefined
          }
        />
        {/* The design's floating toolbar under the photo; while cropping or masking,
            that tool's toolbar takes its place. */}
        <div className="photo-toolbar-strip">
          {crop.open && info ? (
            <CropToolbar tool={crop} straighten={info.straighten} />
          ) : masks.open ? (
            <MaskToolbar tool={masks} zoom={zoomButton} />
          ) : (
            image && (
              <div className="photo-toolbar" role="toolbar" aria-label="Photo tools">
                {zoomButton}
                <span className="toolbar-divider" />
                <button className="tool-button" title="Crop & straighten" onClick={enterCrop}>
                  <CropIcon size={15} />
                  Crop
                </button>
                <button className="tool-button" title="Masks" onClick={enterMasks}>
                  <MaskIcon size={15} />
                  Masks
                </button>
                <button className="tool-button" title={"Before / after (\\)"} aria-pressed={compare.open} onClick={toggleCompare}>
                  <CompareIcon size={15} />
                  Compare
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
        {library.visible.length > 0 && !focus && (
          <Filmstrip library={library} current={currentPath} onOpen={onOpenPhoto} onSync={() => void syncEdits()} syncing={syncing} onExport={onExport} />
        )}
      </div>
      <aside className="panel-right" aria-label="Adjustments" hidden={focus}>
        <Histogram
          histogram={histogram}
          specs={info?.adjustments ?? []}
          recipe={recipe}
          onChange={editor.setRecipe}
          disabled={!image}
          details={exif}
          emptyDetails={image ? "No exposure details" : ""}
        />
        <PresetStrip editor={editor} recipe={recipe} disabled={!image} auto={{ run: () => void runAuto(), busy: autoBusy }} />
        <div className="panel-scroll scroll">
          {info && recipe && (
            <AdjustmentPanel
              specs={info.adjustments}
              mixerSpec={info.mixer}
              curveRegions={info.curveRegions}
              histogram={histogram}
              recipe={recipe}
              onChange={editor.setRecipe}
              disabled={!image}
              temperatureScale={image?.temperatureScale ?? null}
              onAuto={(setting) => void runAutoSetting(setting)}
            />
          )}
          {info && recipe && (
            <PanelSection title="Colour grading" icon={<ColourIcon />} edited={gradingEdited(recipe)} defaultOpen={false}>
              <ColourGradingControls specs={info.grading} recipe={recipe} onChange={editor.setRecipe} disabled={!image} />
            </PanelSection>
          )}
          {info && recipe && (
            <PanelSection title="Calibration" icon={<CalibrationIcon />} edited={calibrationEdited(recipe)} defaultOpen={false}>
              <CalibrationControls specs={info.calibration} recipe={recipe} onChange={editor.setRecipe} disabled={!image} />
            </PanelSection>
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
          {info && recipe && (
            <PanelSection
              title="Selective"
              icon={<MaskIcon />}
              count={masks.masks.length > 0 ? String(masks.masks.length) : undefined}
              edited={masks.masks.length > 0}
              defaultOpen={false}
            >
              <SelectiveControls
                tool={{ ...masks, pick: pickMask }}
                specs={info.mask}
                feather={info.maskFeather}
                density={info.maskDensity}
                disabled={!image}
              />
            </PanelSection>
          )}
          {recipe && (
            <PanelSection
              title="Retouch"
              icon={<RetouchIcon />}
              count={retouch.spots.length + retouch.removals.length > 0 ? String(retouch.spots.length + retouch.removals.length) : undefined}
              edited={retouch.spots.length + retouch.removals.length > 0}
              open={retouchOpen}
              onToggle={toggleRetouch}
            >
              <RetouchControls tool={retouch} disabled={!image} />
            </PanelSection>
          )}
          <PanelSection title="Diagnostics" icon={<DiagnosticsIcon />} defaultOpen={false}>
            <StatsPanel editor={editor} />
          </PanelSection>
        </div>
        <PanelFooter editor={editor} edited={recipe !== null && !isIdentity(recipe)} copy={copyEdits} paste={() => void pasteEdits()} />
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
}: {
  saving: EditSavingDto;
  state: SaveState | null;
  edited: boolean;
}) {
  // Narrow stages show the short form (styles.css); the full note stays on hover.
  const note: { full: string; short?: string } | null =
    saving === "notInLibrary"
      ? { full: "Not saved: this photo isn’t in your library", short: "Not saved" }
      : saving === "newerVersion"
        ? { full: "Edited in a newer version; changes here aren’t saved", short: "Not saved" }
        : state === "saving"
          ? { full: "Saving…" }
          : state === "failed"
            ? { full: "Couldn’t save" }
            : null;
  return (
    <span className="edit-status" aria-live="polite">
      {edited && (
        <span className="edited-label">
          <span className="edited-dot" />
          Edited
        </span>
      )}
      {note && (
        <span className={state === "failed" ? "edit-note failed" : "edit-note"} title={note.short && note.full}>
          {note.short ? (
            <>
              <span className="note-full">{note.full}</span>
              <span className="note-short">{note.short}</span>
            </>
          ) : (
            note.full
          )}
        </span>
      )}
    </span>
  );
}
