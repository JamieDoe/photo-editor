import * as ipc from "../../ipc/client";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { ExportEvent } from "../../ipc/generated/ExportEvent";
import type { IndexEvent } from "../../ipc/generated/IndexEvent";
import type { SelfTestConfigDto } from "../../ipc/generated/SelfTestConfigDto";
import { neutralRecipe } from "../editor/recipe";
import type { DisplayedFrame, Editor } from "../editor/useEditor";

type RenderedFrame = DisplayedFrame;

/**
 * End-to-end self-test, run only when the app is launched with PE_SELF_TEST=<file>.
 * Drives the real UI state (same actions as the sliders) through real IPC and
 * reports UI-side timings to stdout via Rust. See docs/PERFORMANCE.md.
 */
export interface SelfTestDriver {
  editor: () => Editor;
}

/**
 * The next animation frame. The OS stops delivering frames to a hidden or occluded
 * window (or a sleeping display); rather than hang, the run then fails with a report
 * that says so.
 */
const nextFrame = () =>
  new Promise<number>((resolve, reject) => {
    const timer = setTimeout(
      () =>
        reject(
          new Error(
            `animation frames stopped (page visibility: ${document.visibilityState}); the window was probably hidden, covered or on another Space`,
          ),
        ),
      2_000,
    );
    requestAnimationFrame((t) => {
      clearTimeout(timer);
      resolve(t);
    });
  });
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))] ?? 0;
}

function summarise(values: number[]) {
  return { n: values.length, p50: percentile(values, 50), p95: percentile(values, 95), max: Math.max(0, ...values) };
}

async function waitFor<T>(poll: () => T | null | undefined, timeoutMs: number, what: string): Promise<T> {
  const start = performance.now();
  for (;;) {
    const v = poll();
    if (v !== null && v !== undefined) return v;
    if (performance.now() - start > timeoutMs) throw new Error(`timed out waiting for ${what}`);
    await sleep(10);
  }
}

/** Simulates dragging one slider: one value change per animation frame. */
async function drag(driver: SelfTestDriver, base: EditRecipe, steps: number, frames: RenderedFrame[]) {
  const startFrames = frames.length;
  const gaps: number[] = [];
  let last = await nextFrame();
  const t0 = performance.now();
  for (let i = 1; i <= steps; i++) {
    driver.editor().setRecipe({ ...base, exposure: Math.sin(i / 10) * 1.5, temperature: (i % 40) - 20 });
    const now = await nextFrame();
    gaps.push(now - last);
    last = now;
  }
  const durationMs = performance.now() - t0;
  const dragFrames = frames.slice(startFrames).filter((f) => f.info.quality === "interactive");
  return {
    steps,
    durationMs,
    framesShown: dragFrames.length,
    previewFps: (dragFrames.length / durationMs) * 1000,
    roundTripMs: summarise(dragFrames.map((f) => f.info.roundTripMs)),
    rustRenderMs: summarise(dragFrames.filter((f) => !f.frame.cacheHit).map((f) => f.frame.renderMs)),
    // Animation-frame gaps measure UI-thread responsiveness (16.7 ms = perfect at 60 Hz).
    uiFrameGapMs: summarise(gaps),
    frameSize: dragFrames.at(-1) ? `${dragFrames.at(-1)!.frame.width}x${dragFrames.at(-1)!.frame.height}` : null,
  };
}

export async function runSelfTest(config: SelfTestConfigDto, driver: SelfTestDriver): Promise<Record<string, unknown>> {
  const frames: RenderedFrame[] = [];
  // On-screen size of the photo after each frame is painted: every render of an opened
  // photo (quick interactive, then sharp detail) must occupy the same box.
  const shownSizes: Array<{ imageId: number; quality: string; width: number; height: number }> = [];
  const unsubscribe = driver.editor().subscribeFrames((f) => {
    requestAnimationFrame(() =>
      setTimeout(() => {
        const r = document.querySelector(".viewer-canvas")?.getBoundingClientRect();
        if (r) shownSizes.push({ imageId: f.imageId, quality: f.info.quality, width: Math.round(r.width), height: Math.round(r.height) });
      }, 0),
    );
    frames.push(f);
  });
  try {
    const info = await waitFor(() => driver.editor().info, 10_000, "engine info");

    // Baseline animation-frame cadence with no work, to separate environment
    // throttling (e.g. WebView frame-rate limits) from our own cost.
    const baselineGaps: number[] = [];
    let prev = await nextFrame();
    for (let i = 0; i < 60; i++) {
      const now = await nextFrame();
      baselineGaps.push(now - prev);
      prev = now;
    }

    const tOpen = performance.now();
    const summary = await driver.editor().openPath(config.imagePath);
    if (!summary) throw new Error(driver.editor().error?.message ?? "open failed");
    const openIpcMs = performance.now() - tOpen;
    const first = await waitFor(() => frames[0], 10_000, "first frame");
    const firstFrameMs = performance.now() - tOpen;
    const detail = await waitFor(() => frames.find((f) => f.info.quality === "detail"), 10_000, "detail frame");

    const base = driver.editor().recipe!;
    const idleDrag = await drag(driver, base, 120, frames);
    await sleep(400);

    // Toggle between two settled recipes (like a before/after comparison): returning
    // to the first must be served from the preview cache at both qualities.
    const a = { ...base, contrast: 25 };
    const b = { ...base, contrast: -25 };
    driver.editor().setRecipe(a);
    await sleep(400);
    driver.editor().setRecipe(b);
    await sleep(400);
    const beforeReturn = frames.length;
    driver.editor().setRecipe(a);
    await sleep(400);
    const returnFrames = frames.slice(beforeReturn);
    const cacheHitOnReturn = returnFrames.length > 0 && returnFrames.every((f) => f.frame.cacheHit);

    // Export in the background while dragging again.
    const tExport = performance.now();
    const started = await driver.editor().exportImage(config.exportPath);
    if (!started) throw new Error("export did not start");
    const dragDuringExport = await drag(driver, base, 90, frames);
    const finished = await waitFor(
      () => {
        const last: ExportEvent | null | undefined = driver.editor().exportState?.last;
        return last && last.type !== "progress" ? last : null;
      },
      120_000,
      "export",
    );
    const exportWallMs = performance.now() - tExport;

    // Re-open the same file once the app has settled: separates app-startup effects
    // from steady-state open cost, and exercises opening while an image is open.
    const framesBefore = frames.length;
    const reopenSizesStart = shownSizes.length;
    const tReopen = performance.now();
    const reopened = await driver.editor().openPath(config.imagePath);
    if (!reopened) throw new Error(driver.editor().error?.message ?? "re-open failed");
    await waitFor(() => (frames.slice(framesBefore).some((f) => f.imageId === reopened.id) ? true : null), 10_000, "re-open frame");
    const firstRenderMs = performance.now() - tReopen;
    await waitFor(
      () => (frames.slice(framesBefore).some((f) => f.imageId === reopened.id && f.info.quality === "detail") ? true : null),
      10_000,
      "re-open detail frame",
    );
    await sleep(200); // the size of the last frame is sampled after it is painted
    const reopenSizes = shownSizes.slice(reopenSizesStart).filter((s) => s.imageId === reopened.id);
    const last = reopenSizes.at(-1);
    const sizeStable =
      reopenSizes.length >= 2 &&
      last !== undefined &&
      reopenSizes.every((s) => Math.abs(s.width - last.width) <= 1 && Math.abs(s.height - last.height) <= 1);
    const reopen = {
      sizes: reopenSizes.map((s) => `${s.quality}:${s.width}x${s.height}`),
      sizeStable,
      decodeMs: reopened.decodeMs,
      firstRenderMs,
    };

    // Library indexing through the real command and events: first pass, then a
    // rescan that must take the fast path (everything unchanged).
    const indexing = await (async () => {
      const folder = await ipc.selfTestGrantFolder();
      if (!folder) return null;
      const runIndex = async () => {
        let done: IndexEvent | null = null;
        const unlisten = await ipc.onIndexEvent((e) => {
          if (e.type !== "progress") done = e;
        });
        const t0 = performance.now();
        await ipc.indexLibraryFolder(folder);
        const result = await waitFor(() => done, 60_000, "index result");
        unlisten();
        return { event: result, wallMs: performance.now() - t0 };
      };
      const first = await runIndex();
      const rescan = await runIndex();
      return { folder, first, rescan };
    })();
    const indexOk =
      indexing !== null &&
      indexing.first.event.type === "finished" &&
      indexing.first.event.found > 0 &&
      indexing.rescan.event.type === "finished" &&
      indexing.rescan.event.new + indexing.rescan.event.changed === 0;

    // Library thumbnails through the real command: every photo in the folder at once
    // (the self-test cache starts empty), then the test image again from the cache.
    // Each must decode as an image whose long edge is at most 512 px.
    const thumbnails = await (async () => {
      if (!indexing) return null;
      const listing = await ipc.listFolder(indexing.folder);
      const longEdge = async (bytes: ArrayBuffer) => {
        const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/jpeg" }));
        const edge = Math.max(bitmap.width, bitmap.height);
        bitmap.close();
        return edge;
      };
      const t0 = performance.now();
      const all = await Promise.all(listing.photos.map((p) => ipc.libraryThumbnail(p.path)));
      const allMs = performance.now() - t0;
      const edges = await Promise.all(all.map(longEdge));
      const t1 = performance.now();
      const again = await ipc.libraryThumbnail(config.imagePath);
      const cachedMs = performance.now() - t1;
      return {
        photos: listing.photos.length,
        allMs,
        cachedMs,
        maxKb: Math.max(...all.map((b) => b.byteLength)) / 1024,
        longEdges: [...new Set(edges)],
        cachedLongEdge: await longEdge(again),
      };
    })();
    const thumbnailsOk =
      thumbnails !== null &&
      thumbnails.photos > 0 &&
      thumbnails.longEdges.every((e) => e > 0 && e <= 512) &&
      thumbnails.cachedLongEdge > 0;

    // Ratings and flags through the real commands and catalogue: rate and pick the test
    // photo, read it back from the folder listing and the Picks collection, then clear.
    const marks = await (async () => {
      if (!indexing) return null;
      const target = config.imagePath;
      await ipc.setPhotoMarks([target], { type: "rating", stars: 4 });
      const counts = await ipc.setPhotoMarks([target], { type: "flag", flag: "pick" });
      const listed = (await ipc.listFolder(indexing.folder)).photos.find((p) => p.path === target)?.marks ?? null;
      const picks = await ipc.libraryCollection("picks");
      await ipc.setPhotoMarks([target], { type: "rating", stars: 0 });
      const cleared = await ipc.setPhotoMarks([target], { type: "flag", flag: "none" });
      return { counts, listed, inPicks: picks.photos.some((p) => p.path === target), cleared };
    })();
    const marksOk =
      marks !== null &&
      marks.listed?.rating === 4 &&
      marks.listed.flag === "pick" &&
      marks.inPicks &&
      marks.counts.picks >= 1 &&
      marks.cleared.picks === marks.counts.picks - 1;

    // Saved edits through the real commands, catalogue and editor: save a recipe,
    // see it in the listing and the thumbnail, reopen the photo with it, then reset.
    const edits = await (async () => {
      if (!indexing || !thumbnails) return null;
      const target = config.imagePath;
      const before = await ipc.libraryThumbnail(target);
      // Known values: earlier steps left other adjustments changed.
      const original = neutralRecipe(info.recipeVersion);
      const recipe = { ...original, exposure: 1, saturation: -40 };
      const saved = await ipc.saveEdit(target, recipe);
      const listed = (await ipc.listFolder(indexing.folder)).photos.find((p) => p.path === target)?.edited ?? null;
      const after = await ipc.libraryThumbnail(target);
      const reopened = await driver.editor().openPath(target);
      // State updates land on the next render.
      const restored = await waitFor(
        () => (reopened && driver.editor().image?.id === reopened.id ? driver.editor().recipe : null),
        5_000,
        "reopened recipe",
      );
      const reset = await ipc.saveEdit(target, original);
      return {
        edited: saved.edited,
        listed,
        thumbnailChanged: before.byteLength !== after.byteLength || new Uint8Array(before).some((b, i) => b !== new Uint8Array(after)[i]),
        editSaving: reopened?.editSaving ?? null,
        restoredExposure: restored?.exposure ?? null,
        resetEdited: reset.edited,
      };
    })();
    const editsOk =
      edits !== null &&
      edits.edited &&
      edits.listed === true &&
      edits.thumbnailChanged &&
      edits.editSaving === "library" &&
      edits.restoredExposure === 1 &&
      !edits.resetEdited;

    // Quit guard: closing the window while an export runs must be held for
    // confirmation. The final shutdown (self_test_report) then cancels the export.
    let quitRequested: { exportsRunning: number } | null = null;
    const unlistenQuit = await ipc.onQuitRequested((e) => (quitRequested = e));
    const guardExport = await driver.editor().exportImage(config.exportPath);
    await ipc.selfTestRequestClose();
    await waitFor(() => quitRequested, 3_000, "quit confirmation request").catch(() => null);
    unlistenQuit();
    const quitGuard = { exportStarted: guardExport !== null, held: quitRequested !== null };

    const stats = driver.editor().schedulerStats();
    const checks = {
      exportFinished: finished.type === "finished",
      framesDuringDrag: idleDrag.framesShown > 0,
      framesDuringExport: dragDuringExport.framesShown > 0,
      cacheHitOnReturn,
      noSchedulerErrors: stats.errors === 0,
      quitHeldDuringExport: quitGuard.held,
      libraryIndexed: indexOk,
      viewerSizeStableOnOpen: reopen.sizeStable,
      libraryThumbnails: thumbnailsOk,
      ratingsAndFlags: marksOk,
      savedEdits: editsOk,
    };
    // Named so that a failing run explains itself.
    const failed = Object.entries(checks)
      .filter(([, passed]) => !passed)
      .map(([name]) => name);
    return {
      ok: failed.length === 0,
      failed,
      file: summary.fileName,
      codecs: { jpegEncoder: info.jpegEncoder, embeddedJpegDecoder: info.embeddedJpegDecoder },
      camera: summary.camera,
      fullSize: `${summary.fullWidth}x${summary.fullHeight}`,
      pyramid: summary.levels.map(([w, h]) => `${w}x${h}`),
      open: { decodeMs: summary.decodeMs, pyramidMs: summary.pyramidMs, identityMs: summary.identityMs, ipcTotalMs: openIpcMs },
      firstFrame: { ms: firstFrameMs, size: `${first.frame.width}x${first.frame.height}`, quality: first.info.quality },
      firstVisibleMs: firstFrameMs,
      reopen,
      quitGuard,
      indexing,
      thumbnails,
      marks,
      edits,
      detailFrame: { size: `${detail.frame.width}x${detail.frame.height}`, rustRenderMs: detail.frame.renderMs, roundTripMs: detail.info.roundTripMs },
      baselineUiFrameGapMs: summarise(baselineGaps),
      idleDrag,
      cacheHitOnReturn,
      dragDuringExport,
      export: { wallMs: exportWallMs, result: finished },
      scheduler: stats,
      devicePixelRatio: window.devicePixelRatio,
    };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e), scheduler: driver.editor().schedulerStats() };
  } finally {
    unsubscribe();
  }
}
