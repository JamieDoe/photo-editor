import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { PreviewFrame } from "../../ipc/frame";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { EngineInfoDto } from "../../ipc/generated/EngineInfoDto";
import type { ExportEvent } from "../../ipc/generated/ExportEvent";
import type { ImageSummaryDto } from "../../ipc/generated/ImageSummaryDto";
import { Autosaver, type SaveState } from "./autosave";
import { EditHistory, describeChange } from "./history";
import { PreviewScheduler, type FrameInfo, type SchedulerStats } from "./previewScheduler";
import { defaultRecipe } from "./recipe";

/** What the viewer shows: a render of the open image's recipe. The camera's embedded
 * JPEG is never shown in the editor (ADR 0020). */
export interface DisplayedFrame {
  frame: PreviewFrame;
  imageId: number;
  info: FrameInfo;
}

interface RenderedPreview {
  frame: PreviewFrame;
  imageId: number;
}

export type FrameListener = (f: DisplayedFrame) => void;

export interface ExportState {
  jobId: number;
  path: string;
  last: ExportEvent | null;
}

/**
 * Editor state and actions. Owns the preview scheduler; all image work is delegated
 * to Rust through the IPC client.
 */
export function useEditor() {
  const [info, setInfo] = useState<EngineInfoDto | null>(null);
  const [image, setImage] = useState<ImageSummaryDto | null>(null);
  const [recipe, setRecipeState] = useState<EditRecipe | null>(null);
  const [displayed, setDisplayed] = useState<DisplayedFrame | null>(null);
  const [stats, setStats] = useState<SchedulerStats | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  /** Shows an error to the user (logging it first if it did not come from Rust). */
  const fail = useCallback((e: unknown) => void toAppError(e).then(setError), []);
  const [busy, setBusy] = useState(false);
  const [exportState, setExportState] = useState<ExportState | null>(null);
  /** Save state of the open photo's edit; null when its edits are not saved. */
  const [saveState, setSaveState] = useState<SaveState | null>(null);
  /** The latest completed save, so the Library can mark the photo edited or not. */
  const [lastSaved, setLastSaved] = useState<{ path: string; edited: boolean } | null>(null);
  const autosaverRef = useRef<Autosaver | null>(null);
  /** Undo history per photo path, for this session (ADR 0044); the open photo's is
   *  `historyRef`. Each remembers the recipe it ends at, to tell whether it still
   *  applies when the photo is opened again. */
  const historiesRef = useRef(new Map<string, { history: EditHistory; recipe: EditRecipe }>());
  const historyRef = useRef<{ history: EditHistory; recipe: EditRecipe } | null>(null);
  /** What Undo and Redo would change ("Exposure"), or null. */
  const [undoLabel, setUndoLabel] = useState<string | null>(null);
  const [redoLabel, setRedoLabel] = useState<string | null>(null);

  const imageRef = useRef<ImageSummaryDto | null>(null);
  const recipeRef = useRef<EditRecipe | null>(null);
  const targetEdgeRef = useRef(1600);
  const listenersRef = useRef(new Set<FrameListener>());
  // Latest event per export job. Events can arrive before export_image resolves.
  const exportEventsRef = useRef(new Map<number, ExportEvent>());
  // Increments per open, so a slower, older open never replaces a newer one.
  const openGenRef = useRef(0);

  // The scheduler lives exactly as long as the mounted component. Created in an
  // effect (not useMemo) so React StrictMode's mount -> unmount -> mount cycle in
  // development gets a fresh scheduler instead of a disposed one.
  const schedulerRef = useRef<PreviewScheduler<RenderedPreview> | null>(null);

  useEffect(() => {
    const scheduler = new PreviewScheduler<RenderedPreview>({
      render: async (r, quality) => {
        const img = imageRef.current;
        if (!img) throw ipc.staleError();
        const frame = await ipc.renderPreview({ imageId: img.id, recipe: r, quality, targetLongEdge: targetEdgeRef.current });
        // A different image was opened while this rendered.
        if (imageRef.current?.id !== img.id) throw ipc.staleError();
        return { frame, imageId: img.id };
      },
      isCancellation: ipc.isCancellation,
      onFrame: (r, frameInfo) => show({ frame: r.frame, imageId: r.imageId, info: frameInfo }),
      onError: fail,
      requestFrame: (cb) => requestAnimationFrame(cb),
      setTimer: (cb, ms) => window.setTimeout(cb, ms),
      clearTimer: (h) => window.clearTimeout(h),
      now: () => performance.now(),
    });
    schedulerRef.current = scheduler;
    const timer = window.setInterval(() => setStats(scheduler.stats()), 250);
    return () => {
      window.clearInterval(timer);
      scheduler.dispose();
      if (schedulerRef.current === scheduler) schedulerRef.current = null;
    };
  }, []);

  useEffect(() => {
    const saver = new Autosaver({
      save: (path, r) => ipc.saveEdit(path, r),
      setTimer: (cb, ms) => window.setTimeout(cb, ms),
      clearTimer: (h) => window.clearTimeout(h),
      onState: (path, state, edited, e) => {
        if (imageRef.current?.path === path) setSaveState(state);
        if (state === "saved" && edited !== null) setLastSaved({ path, edited });
        if (state === "failed") fail(e);
      },
    });
    autosaverRef.current = saver;
    return () => {
      void saver.flush(); // don't lose the last change when the editor goes away
      if (autosaverRef.current === saver) autosaverRef.current = null;
    };
  }, []);

  // A pointer held down (a drag, a brush stroke) makes its changes one step. It ends
  // after the pointer-up's own handlers, which may make the last change.
  useEffect(() => {
    const begin = () => historyRef.current?.history.beginGesture();
    const end = () => window.setTimeout(() => historyRef.current?.history.endGesture(), 0);
    window.addEventListener("pointerdown", begin, true);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerdown", begin, true);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, []);

  const syncHistoryLabels = () => {
    const h = historyRef.current?.history;
    setUndoLabel(h?.undoLabel ?? null);
    setRedoLabel(h?.redoLabel ?? null);
  };

  function show(d: DisplayedFrame) {
    setDisplayed(d);
    listenersRef.current.forEach((l) => l(d));
  }

  useEffect(() => {
    ipc.engineInfo().then(setInfo, fail);
    const unlisten = ipc
      .onExportEvent((event) => {
        exportEventsRef.current.set(event.jobId, event);
        setExportState((s) => (s && s.jobId === event.jobId ? { ...s, last: event } : s));
        if (event.type === "failed") setError({ message: event.error.message, reference: event.error.reference });
      })
      .catch((e: unknown) => {
        fail(e);
        return () => {};
      });
    return () => void unlisten.then((u) => u());
  }, []);

  /** How the recipe is rendered: normally as it is; the crop tool renders the whole
   *  straightened view instead, to draw the crop over it. */
  const viewTransformRef = useRef<((r: EditRecipe) => EditRecipe) | null>(null);
  const toRender = (r: EditRecipe) => (viewTransformRef.current ? viewTransformRef.current(r) : r);

  /** Shows `r` without saving it (opening a photo). */
  const applyRecipe = useCallback((r: EditRecipe) => {
    recipeRef.current = r;
    setRecipeState(r);
    schedulerRef.current?.request(toRender(r));
  }, []);

  /** Renders recipes through `transform` (or as they are, with null). */
  const setViewTransform = useCallback((transform: ((r: EditRecipe) => EditRecipe) | null) => {
    viewTransformRef.current = transform;
    if (imageRef.current && recipeRef.current) schedulerRef.current?.request(toRender(recipeRef.current));
  }, []);

  /** Shows `r` as the photographer's edit, and saves it if the photo is in the library. */
  const commitRecipe = useCallback(
    (r: EditRecipe) => {
      applyRecipe(r);
      if (historyRef.current) historyRef.current.recipe = r;
      const img = imageRef.current;
      if (img?.editSaving === "library") autosaverRef.current?.schedule(img.path, r);
    },
    [applyRecipe],
  );

  /** The photographer changed the edit: a step in its history. */
  const setRecipe = useCallback(
    (r: EditRecipe) => {
      const before = recipeRef.current;
      if (before && historyRef.current) historyRef.current.history.record(before, r, performance.now());
      commitRecipe(r);
      syncHistoryLabels();
    },
    [commitRecipe],
  );

  /** Undoes (-1) or redoes (+1) the last step of the open photo's edit. */
  const stepHistory = useCallback(
    (direction: -1 | 1) => {
      const h = historyRef.current?.history;
      const current = recipeRef.current;
      if (!h || !current) return;
      const next = direction < 0 ? h.undo(current) : h.redo(current);
      if (next) commitRecipe(next);
      syncHistoryLabels();
    },
    [commitRecipe],
  );
  const undo = useCallback(() => stepHistory(-1), [stepHistory]);
  const redo = useCallback(() => stepHistory(1), [stepHistory]);

  const adopt = useCallback(
    (summary: ImageSummaryDto | null) => {
      if (!summary || !info) return summary;
      imageRef.current = summary;
      setImage(summary);
      setError(null);
      // The saved edit (if any) is the starting point, so the first render shows it.
      const start = summary.savedRecipe ?? defaultRecipe(info.recipeVersion, info.adjustments);
      applyRecipe(start);
      historyRef.current = historyFor(summary.path, start);
      syncHistoryLabels();
      setSaveState(summary.editSaving === "library" ? "saved" : null);
      return summary;
    },
    [info, applyRecipe],
  );

  /** See `historyForPhoto`; steps are named after the sliders' labels. */
  const historyFor = (path: string, start: EditRecipe) =>
    historyForPhoto(historiesRef.current, path, start, new Map((info?.adjustments ?? []).map((a) => [a.key, a.label])));

  /**
   * Opens a photo. The current photo stays on screen (the viewer dims it) until the
   * new one's first render arrives; a newer open supersedes this one.
   */
  const runOpen = useCallback(
    async (open: () => Promise<ImageSummaryDto | null>) => {
      // The previous photo's last change is saved before another photo takes over.
      void autosaverRef.current?.flush();
      const gen = ++openGenRef.current;
      setBusy(true);
      try {
        const summary = await open();
        if (gen !== openGenRef.current) return null; // superseded by a newer open
        return adopt(summary);
      } catch (e) {
        if (!ipc.isCancellation(e)) fail(e);
        return null;
      } finally {
        if (gen === openGenRef.current) setBusy(false);
      }
    },
    [adopt],
  );

  const openDialog = useCallback(() => runOpen(ipc.openImageDialog), [runOpen]);
  const openPath = useCallback((path: string) => runOpen(() => ipc.openImagePath(path)), [runOpen]);

  const exportImage = useCallback(
    async (destination: string | null = null) => {
      const img = imageRef.current;
      if (!img || !recipe) return null;
      try {
        const started = await ipc.exportImage({ imageId: img.id, recipe, destination });
        if (started) {
          const early = exportEventsRef.current.get(started.jobId) ?? null;
          setExportState({ jobId: started.jobId, path: started.path, last: early });
        }
        return started;
      } catch (e) {
        fail(e);
        return null;
      }
    },
    [recipe],
  );

  const setTargetLongEdge = useCallback(
    (edge: number) => {
      const rounded = Math.max(64, Math.round(edge));
      if (Math.abs(rounded - targetEdgeRef.current) < 32) return;
      targetEdgeRef.current = rounded;
      // Read the recipe from a ref so this callback stays stable across edits (the
      // viewer's ResizeObserver subscribes to it).
      if (imageRef.current && recipeRef.current) schedulerRef.current?.request(toRender(recipeRef.current));
    },
    [],
  );

  /** Renders `r` for the before/after comparison (ADR 0045), in its own slot so it
   *  and the edit's renders never cancel each other. Rejects with a cancellation if
   *  another photo was opened meanwhile. */
  const renderCompare = useCallback(async (r: EditRecipe, longEdge: number) => {
    const img = imageRef.current;
    if (!img) throw ipc.staleError();
    const frame = await ipc.renderPreview({
      imageId: img.id,
      recipe: r,
      quality: "detail",
      targetLongEdge: Math.max(64, Math.round(longEdge)),
      slot: "compare",
    });
    if (imageRef.current?.id !== img.id) throw ipc.staleError();
    return frame;
  }, []);

  const subscribeFrames = useCallback((l: FrameListener) => {
    listenersRef.current.add(l);
    return () => listenersRef.current.delete(l);
  }, []);

  return {
    info,
    image,
    recipe,
    displayed,
    stats,
    error,
    busy,
    exportState,
    setRecipe,
    /** Undo and redo (ADR 0044), and what each would change (null: nothing). */
    undo,
    redo,
    undoLabel,
    redoLabel,
    resetRecipe: () => info && setRecipe(defaultRecipe(info.recipeVersion, info.adjustments)),
    saveState,
    lastSaved,
    openDialog,
    openPath,
    exportImage,
    setTargetLongEdge,
    setViewTransform,
    subscribeFrames,
    renderCompare,
    schedulerStats: (): SchedulerStats =>
      schedulerRef.current?.stats() ?? { requested: 0, shown: 0, superseded: 0, stale: 0, errors: 0 },
    clearError: () => setError(null),
  };
}

export type Editor = ReturnType<typeof useEditor>;

/** Photos whose history is kept for the session: the least recently opened go. */
const MAX_HISTORIES = 30;

/**
 * The open photo's history, continuing the one from when it was last open this
 * session if the photo's edit is still what that history ended at (otherwise the edit
 * changed elsewhere, and history starts again from here).
 */
function historyForPhoto(
  histories: Map<string, { history: EditHistory; recipe: EditRecipe }>,
  path: string,
  start: EditRecipe,
  labels: ReadonlyMap<string, string>,
): { history: EditHistory; recipe: EditRecipe } {
  const kept = histories.get(path);
  histories.delete(path);
  const entry =
    kept && JSON.stringify(kept.recipe) === JSON.stringify(start)
      ? kept
      : { history: new EditHistory((keys) => describeChange(keys, labels)), recipe: start };
  histories.set(path, entry);
  if (histories.size > MAX_HISTORIES) histories.delete(histories.keys().next().value!);
  return entry;
}
