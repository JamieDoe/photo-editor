import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { PreviewFrame } from "../../ipc/frame";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { EngineInfoDto } from "../../ipc/generated/EngineInfoDto";
import type { ExportEvent } from "../../ipc/generated/ExportEvent";
import type { ImageSummaryDto } from "../../ipc/generated/ImageSummaryDto";
import { Autosaver, type SaveState } from "./autosave";
import { PreviewScheduler, type FrameInfo, type SchedulerStats } from "./previewScheduler";
import { defaultRecipe } from "./recipe";

/** What the viewer shows: a render of the recipe, or the embedded camera preview
 * shown while the file decodes. */
export type DisplayedFrame =
  | { source: "render"; frame: PreviewFrame; imageId: number; info: FrameInfo }
  | { source: "embedded"; frame: PreviewFrame; sinceOpenMs: number };

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

  const imageRef = useRef<ImageSummaryDto | null>(null);
  const recipeRef = useRef<EditRecipe | null>(null);
  const targetEdgeRef = useRef(1600);
  const listenersRef = useRef(new Set<FrameListener>());
  // Latest event per export job. Events can arrive before export_image resolves.
  const exportEventsRef = useRef(new Map<number, ExportEvent>());
  // Increments per open. While an open is pending, frames of the previous image are
  // obsolete and must not replace the new file's embedded preview.
  const openGenRef = useRef(0);
  const pendingOpenRef = useRef<number | null>(null);
  const adoptedGenRef = useRef(0);

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
        // A different image was opened (or is opening) while this rendered.
        if (imageRef.current?.id !== img.id || pendingOpenRef.current !== null) throw ipc.staleError();
        return { frame, imageId: img.id };
      },
      isCancellation: ipc.isCancellation,
      onFrame: (r, frameInfo) => show({ source: "render", frame: r.frame, imageId: r.imageId, info: frameInfo }),
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

  /** Shows `r` without saving it (opening a photo). */
  const applyRecipe = useCallback((r: EditRecipe) => {
    recipeRef.current = r;
    setRecipeState(r);
    schedulerRef.current?.request(r);
  }, []);

  /** The photographer changed the edit: show it, and save it if the photo is in the library. */
  const setRecipe = useCallback(
    (r: EditRecipe) => {
      applyRecipe(r);
      const img = imageRef.current;
      if (img?.editSaving === "library") autosaverRef.current?.schedule(img.path, r);
    },
    [applyRecipe],
  );

  const adopt = useCallback(
    (summary: ImageSummaryDto | null) => {
      if (!summary || !info) return summary;
      pendingOpenRef.current = null;
      imageRef.current = summary;
      setImage(summary);
      setError(null);
      // The saved edit (if any) is the starting point, so the first render shows it.
      applyRecipe(summary.savedRecipe ?? defaultRecipe(info.recipeVersion, info.adjustments));
      setSaveState(summary.editSaving === "library" ? "saved" : null);
      return summary;
    },
    [info, applyRecipe],
  );

  const runOpen = useCallback(
    async (open: (onPreview: ipc.PreviewHandler) => Promise<ImageSummaryDto | null>) => {
      // The previous photo's last change is saved before another photo takes over.
      void autosaverRef.current?.flush();
      const gen = ++openGenRef.current;
      const started = performance.now();
      const onPreview: ipc.PreviewHandler = (frame) => {
        // Only for the latest open, and only before it is adopted: the channel message
        // can arrive after the open's response, and must never replace a real render.
        if (gen !== openGenRef.current || adoptedGenRef.current === gen) return;
        pendingOpenRef.current = gen;
        show({ source: "embedded", frame, sinceOpenMs: performance.now() - started });
      };
      setBusy(true);
      try {
        const summary = await open(onPreview);
        if (gen !== openGenRef.current) return null; // superseded by a newer open
        if (!summary) {
          restoreAfterFailedOpen(gen);
          return null;
        }
        adoptedGenRef.current = gen;
        return adopt(summary);
      } catch (e) {
        if (gen === openGenRef.current) restoreAfterFailedOpen(gen);
        if (!ipc.isCancellation(e)) fail(e);
        return null;
      } finally {
        if (gen === openGenRef.current) setBusy(false);
      }
    },
    [adopt],
  );

  /** If an open showed an embedded preview but then failed, show the previous image again. */
  function restoreAfterFailedOpen(gen: number) {
    if (pendingOpenRef.current !== gen) return;
    pendingOpenRef.current = null;
    if (imageRef.current && recipeRef.current) schedulerRef.current?.request(recipeRef.current);
    else setDisplayed(null);
  }

  const openDialog = useCallback(() => runOpen(ipc.openImageDialog), [runOpen]);
  const openPath = useCallback((path: string) => runOpen((onPreview) => ipc.openImagePath(path, onPreview)), [runOpen]);

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
      if (imageRef.current && recipeRef.current) schedulerRef.current?.request(recipeRef.current);
    },
    [],
  );

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
    resetRecipe: () => info && setRecipe(defaultRecipe(info.recipeVersion, info.adjustments)),
    saveState,
    lastSaved,
    openDialog,
    openPath,
    exportImage,
    setTargetLongEdge,
    subscribeFrames,
    schedulerStats: (): SchedulerStats =>
      schedulerRef.current?.stats() ?? { requested: 0, shown: 0, superseded: 0, stale: 0, errors: 0 },
    clearError: () => setError(null),
  };
}

export type Editor = ReturnType<typeof useEditor>;
