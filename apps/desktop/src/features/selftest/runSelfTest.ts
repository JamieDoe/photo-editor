import * as ipc from "../../ipc/client";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { ExportEvent } from "../../ipc/generated/ExportEvent";
import type { IndexEvent } from "../../ipc/generated/IndexEvent";
import type { SelfTestConfigDto } from "../../ipc/generated/SelfTestConfigDto";
import { fitCropFor } from "../editor/cropGeometry";
import { mixerOf, neutralRecipe } from "../editor/recipe";
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
    driver.editor().setRecipe({ ...base, exposure: Math.sin(i / 10) * 1.5, temperature: (i % 40) - 20, tint: (i % 30) - 15, vibrance: 30, clarity: 25, dehaze: 20, noiseReduction: 30, vignette: -25, grain: 20, texture: (i % 20) - 10, mixer: { ...mixerOf(base), blue: { hue: 10, saturation: 20, luminance: -(i % 50) } } });
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

    // Tone curve (ADR 0037) through the real renderer: a curve lifting the midtones
    // brightens the frame (its mean luminance, from the frame's histogram).
    const meanOf = (f: RenderedFrame, plane: "luma" | "red" | "green") => {
      const h = f.frame.histogram;
      if (!h) return NaN;
      let sum = 0;
      let count = 0;
      h[plane].forEach((n, v) => {
        sum += n * v;
        count += n;
      });
      return sum / count;
    };
    const meanLuma = (f: RenderedFrame) => meanOf(f, "luma");
    /** Sets `recipe` and waits for a frame requested after it (the size of `like`,
     *  when given: quick and detail renders differ). Frames still in flight for the
     *  recipe before are never taken for it. */
    const show = (recipe: EditRecipe, what: string, like?: RenderedFrame | null) => {
      const mark = driver.editor().schedulerStats().requested;
      driver.editor().setRecipe(recipe);
      if (like === null) return Promise.resolve(null);
      return waitFor(
        () =>
          frames.find(
            (f) =>
              f.info.seq > mark &&
              (like === undefined || (f.frame.width === like.frame.width && f.frame.height === like.frame.height)),
          ) ?? null,
        10_000,
        what,
      ).catch(() => null);
    };
    const beforeCurve = driver.editor().recipe!;
    const curved = await show(
      {
        ...beforeCurve,
        pointCurve: [
          [0, 0],
          [0.5, 0.75],
          [1, 1],
        ],
      },
      "tone curve frame",
    );
    // Against the recipe without the curve, at the same size.
    const baseline = await show(beforeCurve, "uncurved frame", curved);
    // A red curve (ADR 0038) lifts red, not green. The colour controls (which run
    // after the curve and mix channels) are neutral for this check.
    const colourNeutral = { ...beforeCurve, saturation: 0, vibrance: 0, mixer: undefined };
    const reddened = await show(
      {
        ...colourNeutral,
        channelCurves: {
          red: [
            [0, 0],
            [0.5, 0.75],
            [1, 1],
          ],
        },
      },
      "red curve frame",
    );
    const restored = await show(colourNeutral, "restored frame", reddened);
    driver.editor().setRecipe(beforeCurve);
    const lift = (f: RenderedFrame | null, plane: "red" | "green") =>
      f && restored ? Math.round((meanOf(f, plane) - meanOf(restored, plane)) * 10) / 10 : null;
    const toneCurve = {
      baselineMeanLuma: baseline ? Math.round(meanLuma(baseline)) : null,
      curvedMeanLuma: curved ? Math.round(meanLuma(curved)) : null,
      renderMs: curved?.frame.renderMs ?? null,
      redCurveLift: { red: lift(reddened, "red"), green: lift(reddened, "green") },
    };
    const toneCurveOk =
      toneCurve.baselineMeanLuma !== null &&
      toneCurve.curvedMeanLuma !== null &&
      toneCurve.curvedMeanLuma > toneCurve.baselineMeanLuma + 10 &&
      toneCurve.redCurveLift.red !== null &&
      toneCurve.redCurveLift.green !== null &&
      toneCurve.redCurveLift.red > 10 &&
      Math.abs(toneCurve.redCurveLift.green) < 3;

    // Undo and redo (ADR 0044): three exposures while a pointer is held down are one
    // step. Undoing it renders the photo as before the drag, redoing it as dragged.
    const history = await (async () => {
      const ed = () => driver.editor();
      // The recipe set just before, once React has shown it.
      const before = await waitFor(() => (ed().recipe === beforeCurve ? beforeCurve : null), 5_000, "recipe before the drag");
      const sameSize = (f: RenderedFrame) =>
        baseline !== null && f.frame.width === baseline.frame.width && f.frame.height === baseline.frame.height;
      /** The first frame (at the baseline's size) of what `act` requests, once React
       *  has shown the change too. */
      const frameOf = async (act: () => void, what: string) => {
        const mark = ed().schedulerStats().requested;
        act();
        const frame = await waitFor(() => frames.find((f) => f.info.seq > mark && sameSize(f)) ?? null, 10_000, what).catch(
          () => null,
        );
        await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
        return frame;
      };
      window.dispatchEvent(new PointerEvent("pointerdown"));
      ed().setRecipe({ ...before, exposure: before.exposure + 0.3 });
      ed().setRecipe({ ...before, exposure: before.exposure + 0.6 });
      const draggedFrame = await frameOf(() => ed().setRecipe({ ...before, exposure: before.exposure + 0.9 }), "dragged frame");
      window.dispatchEvent(new PointerEvent("pointerup"));
      await new Promise((r) => setTimeout(r, 20));
      const dragged = ed().recipe!;
      const label = ed().undoLabel;
      const undoneFrame = await frameOf(() => ed().undo(), "undone frame");
      const undone = ed().recipe!;
      const redoLabel = ed().redoLabel;
      const redoneFrame = await frameOf(() => ed().redo(), "redone frame");
      const redone = ed().recipe!;
      const luma = (f: RenderedFrame | null) => (f ? Math.round(meanLuma(f) * 100) / 100 : null);
      return {
        label,
        redoLabel,
        undoneMatchesRecipe: JSON.stringify(undone) === JSON.stringify(before),
        redoneMatchesRecipe: redone === dragged,
        meanLuma: { before: luma(baseline), dragged: luma(draggedFrame), undone: luma(undoneFrame), redone: luma(redoneFrame) },
      };
    })();
    driver.editor().setRecipe(beforeCurve);
    const m = history.meanLuma;
    const historyOk =
      history.label === "Exposure" &&
      history.redoLabel === "Exposure" &&
      history.undoneMatchesRecipe &&
      history.redoneMatchesRecipe &&
      m.before !== null &&
      m.dragged !== null &&
      m.undone !== null &&
      m.redone !== null &&
      m.dragged > m.before + 10 &&
      Math.abs(m.undone - m.before) < 0.5 &&
      Math.abs(m.redone - m.dragged) < 0.5;

    // Auto level through the real command (ADR 0033): an angle or null, quickly.
    const tLevel = performance.now();
    let levelError: string | null = null;
    const levelAngle = await ipc.autoLevel(driver.editor().image!.id).catch((e: unknown) => {
      levelError = e instanceof Error ? e.message : JSON.stringify(e);
      return undefined;
    });
    const autoLevel = { angle: levelAngle ?? null, ms: Math.round(performance.now() - tLevel), error: levelError };
    const autoLevelOk = levelAngle !== undefined && autoLevel.ms < 2000;

    // Crop through the real pipeline (ADR 0032): the frame shows the cropped part and
    // reports its full-resolution size, which the viewer's box follows.
    const [fullW, fullH] = [summary.fullWidth, summary.fullHeight];
    const beforeCrop = driver.editor().recipe!;
    const framesBeforeCrop = frames.length;
    driver.editor().setRecipe({
      ...beforeCrop,
      geometry: { straighten: 0, crop: { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, aspect: "free", vertical: 0, horizontal: 0, rotation: 0, flip: false },
    });
    const cropped = await waitFor(
      () => frames.slice(framesBeforeCrop).find((f) => f.frame.fullWidth === Math.round(fullW / 2)) ?? null,
      10_000,
      "cropped frame",
    ).catch(() => null);
    driver.editor().setRecipe(beforeCrop);
    const crop = {
      expected: `${Math.round(fullW / 2)}x${Math.round(fullH / 2)}`,
      fullSize: cropped ? `${cropped.frame.fullWidth}x${cropped.frame.fullHeight}` : null,
      frameSize: cropped ? `${cropped.frame.width}x${cropped.frame.height}` : null,
    };
    const cropOk =
      cropped !== null &&
      cropped.frame.fullHeight === Math.round(fullH / 2) &&
      Math.abs(cropped.frame.width / cropped.frame.height - fullW / fullH) < 0.02;

    // A mask (ADR 0040): -1 EV over the top, fading out by the middle. Compared with
    // the same frame without it, rendered at the same size: the top rows darker, the
    // bottom rows unchanged.
    const maskedRecipe = {
      ...beforeCrop,
      masks: [
        {
          id: 1,
          shape: { kind: "linear" as const, start: [0.5, 0] as [number, number], end: [0.5, 0.5] as [number, number] },
          adjustments: { exposure: -1, warmth: 0, clarity: 0 },
        },
      ],
    };
    const maskedFrame = await show(maskedRecipe, "masked frame");
    const unmaskedFrame = await show(beforeCrop, "unmasked frame", maskedFrame);
    // Mean of rows [from, to) as fractions of the height (RGB, 0..255).
    const rowsMean = (f: RenderedFrame, from: number, to: number) => {
      const { width, height, pixels } = f.frame;
      let sum = 0;
      let n = 0;
      for (let y = Math.floor(from * height); y < Math.floor(to * height); y++) {
        for (let x = 0; x < width; x++) {
          const i = (y * width + x) * 4;
          sum += pixels[i]! + pixels[i + 1]! + pixels[i + 2]!;
          n += 3;
        }
      }
      return sum / n;
    };
    const mask =
      maskedFrame && unmaskedFrame
        ? {
            topDarker: Math.round((rowsMean(unmaskedFrame, 0, 0.1) - rowsMean(maskedFrame, 0, 0.1)) * 10) / 10,
            bottomChange: Math.round((rowsMean(maskedFrame, 0.6, 1) - rowsMean(unmaskedFrame, 0.6, 1)) * 100) / 100,
            renderMs: maskedFrame.frame.renderMs,
          }
        : null;
    const linearMaskOk = mask !== null && mask.topDarker > 5 && Math.abs(mask.bottomChange) < 0.5;
    // An inverted radial (ADR 0041), -1 EV outside a circle in the middle: the edges
    // darker, the middle unchanged.
    const radialFrame = await show(
      {
        ...beforeCrop,
        masks: [
          {
            id: 1,
            shape: { kind: "radial", centre: [0.5, 0.5], radius: [0.2, 0.2], angle: 0, feather: 30 },
            invert: true,
            adjustments: { exposure: -1, warmth: 0, clarity: 0 },
          },
        ],
      },
      "radial mask frame",
    );
    const plainFrame = await show(beforeCrop, "frame without the radial mask", radialFrame);
    // Mean of a band of rows' middle columns, or the whole rows.
    const patchMean = (f: RenderedFrame, y0: number, y1: number, x0: number, x1: number) => {
      const { width, height, pixels } = f.frame;
      let sum = 0;
      let n = 0;
      for (let y = Math.floor(y0 * height); y < Math.floor(y1 * height); y++) {
        for (let x = Math.floor(x0 * width); x < Math.floor(x1 * width); x++) {
          const i = (y * width + x) * 4;
          sum += pixels[i]! + pixels[i + 1]! + pixels[i + 2]!;
          n += 3;
        }
      }
      return sum / n;
    };
    const radial =
      radialFrame && plainFrame
        ? {
            edgesDarker: Math.round((patchMean(plainFrame, 0, 0.1, 0, 1) - patchMean(radialFrame, 0, 0.1, 0, 1)) * 10) / 10,
            middleChange:
              Math.round((patchMean(radialFrame, 0.45, 0.55, 0.45, 0.55) - patchMean(plainFrame, 0.45, 0.55, 0.45, 0.55)) * 100) / 100,
            renderMs: radialFrame.frame.renderMs,
          }
        : null;
    // A brush stroke (ADR 0042), -1 EV across the middle: that band darker, the top
    // rows unchanged.
    const brushedFrame = await show(
      {
        ...beforeCrop,
        masks: [
          {
            id: 1,
            shape: {
              kind: "brush",
              strokes: [{ size: 0.05, feather: 40, flow: 100, points: [[0.05, 0.5], [0.5, 0.52], [0.95, 0.5]] }],
            },
            adjustments: { exposure: -1, warmth: 0, clarity: 0 },
          },
        ],
      },
      "brush mask frame",
    );
    const unbrushedFrame = await show(beforeCrop, "frame without the brush mask", brushedFrame);
    // The same stroke with its middle erased: the middle of the band as without the
    // mask, its left still darker.
    const erasedFrame = await show(
      {
        ...beforeCrop,
        masks: [
          {
            id: 1,
            shape: {
              kind: "brush",
              strokes: [
                { size: 0.05, feather: 40, flow: 100, points: [[0.05, 0.5], [0.5, 0.52], [0.95, 0.5]] },
                { erase: true, size: 0.08, feather: 20, flow: 100, points: [[0.5, 0.3], [0.5, 0.7]] },
              ],
            },
            adjustments: { exposure: -1, warmth: 0, clarity: 0 },
          },
        ],
      },
      "erased brush frame",
      brushedFrame,
    );
    const brush =
      brushedFrame && unbrushedFrame
        ? {
            bandDarker: Math.round((patchMean(unbrushedFrame, 0.48, 0.53, 0.1, 0.9) - patchMean(brushedFrame, 0.48, 0.53, 0.1, 0.9)) * 10) / 10,
            topChange: Math.round((patchMean(brushedFrame, 0, 0.2, 0, 1) - patchMean(unbrushedFrame, 0, 0.2, 0, 1)) * 100) / 100,
            erasedMiddleChange: erasedFrame
              ? Math.round((patchMean(erasedFrame, 0.48, 0.53, 0.48, 0.52) - patchMean(unbrushedFrame, 0.48, 0.53, 0.48, 0.52)) * 100) / 100
              : null,
            erasedLeftDarker: erasedFrame
              ? Math.round((patchMean(unbrushedFrame, 0.48, 0.53, 0.1, 0.3) - patchMean(erasedFrame, 0.48, 0.53, 0.1, 0.3)) * 10) / 10
              : null,
            renderMs: brushedFrame.frame.renderMs,
          }
        : null;
    // Painting and erasing through the mask UI itself (ADR 0042): the Edit view, the
    // Masks button, Add Brush, and pointer events on the photo, as the photographer
    // paints. Records how long each stroke's frames take and what the final render
    // shows where the erase went.
    const uiBrush = await (async () => {
      const byText = (sel: string, text: string) =>
        [...document.querySelectorAll<HTMLElement>(sel)].find((e) => e.textContent?.trim() === text);
      driver.editor().setRecipe({ ...beforeCrop, masks: undefined });
      byText("button", "Edit")?.click();
      await nextFrame();
      await nextFrame();
      document.querySelector<HTMLElement>('button[title="Masks"]')?.click();
      await nextFrame();
      byText(".mask-toolbar button", "Brush")?.click();
      await nextFrame();
      await nextFrame();
      const added = driver.editor().recipe!;
      driver.editor().setRecipe({
        ...added,
        masks: added.masks?.map((m) => ({ ...m, adjustments: { ...m.adjustments, exposure: -1 } })),
      });
      await nextFrame();
      const surface = document.querySelector<HTMLElement>(".brush-surface");
      if (!surface) return { error: "no brush surface" };
      const box = surface.getBoundingClientRect();
      const fire = (type: string, u: number, v: number, alt: boolean) =>
        surface.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            clientX: box.left + u * box.width,
            clientY: box.top + v * box.height,
            pointerId: 1,
            isPrimary: true,
            button: 0,
            buttons: type === "pointerup" ? 0 : 1,
            pointerType: "mouse",
            altKey: alt,
          }),
        );
      const stroke = async (from: [number, number], to: [number, number], alt: boolean, moves = 40) => {
        const framesBefore = frames.length;
        const gaps: number[] = [];
        let last = await nextFrame();
        fire("pointerdown", from[0], from[1], alt);
        for (let i = 1; i <= moves; i++) {
          const t = i / moves;
          fire("pointermove", from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t, alt);
          const now = await nextFrame();
          gaps.push(now - last);
          last = now;
        }
        fire("pointerup", to[0], to[1], alt);
        const shown = frames.slice(framesBefore);
        return {
          uiFrameGapMs: summarise(gaps),
          framesShown: shown.length,
          roundTripMs: summarise(shown.map((f) => f.info.roundTripMs)),
          rustRenderMs: summarise(shown.map((f) => f.frame.renderMs)),
          frameSize: shown.at(-1) ? `${shown.at(-1)!.frame.width}x${shown.at(-1)!.frame.height}` : null,
        };
      };
      const paint = await stroke([0.05, 0.5], [0.95, 0.5], false);
      const erase = await stroke([0.5, 0.2], [0.5, 0.8], true);
      // Let the last change settle (the detail render follows 180 ms after it).
      await new Promise((r) => setTimeout(r, 1200));
      const painted = driver.editor().recipe!;
      const mask = painted.masks?.[0];
      const strokes = mask?.shape.kind === "brush" ? mask.shape.strokes : [];
      const final = frames.at(-1) ?? null;
      // Then heavier painting, for its timing: a large brush, nine more long strokes
      // (every third an erase, which used to slow each later one down).
      for (let k = 0; k < 5; k++) window.dispatchEvent(new KeyboardEvent("keydown", { key: "]" }));
      await nextFrame();
      const sizes: Array<ReturnType<typeof summarise>> = [];
      let lastHeavy: Awaited<ReturnType<typeof stroke>> | null = null;
      for (let k = 0; k < 9; k++) {
        const y = 0.1 + k * 0.1;
        lastHeavy = await stroke([0.05, y], [0.95, y + 0.05], k % 3 === 2, 80);
        sizes.push(lastHeavy.uiFrameGapMs);
      }
      byText(".mask-toolbar button", "Done")?.click();
      const plain = await show({ ...painted, masks: undefined }, "frame without the painted mask", final);
      const middle = (f: RenderedFrame) => patchMean(f, 0.48, 0.53, 0.47, 0.53);
      const left = (f: RenderedFrame) => patchMean(f, 0.48, 0.53, 0.1, 0.3);
      driver.editor().setRecipe(beforeCrop);
      return {
        strokes: strokes.map((s) => ({ erase: s.erase ?? false, points: s.points.length })),
        paint,
        erase,
        heavyGapsByStroke: sizes.map((s) => `${s.p50}/${s.p95}`),
        lastHeavy,
        erasedMiddleChange: final && plain ? Math.round((middle(final) - middle(plain)) * 100) / 100 : null,
        paintedLeftDarker: final && plain ? Math.round((left(plain) - left(final)) * 10) / 10 : null,
      };
    })();

    // Shapes combined (ADR 0043): -1 EV over the top, less a hard circle at its
    // middle. Inside the circle unchanged, beside it darker.
    const combinedFrame = await show(
      {
        ...beforeCrop,
        masks: [
          {
            id: 1,
            shape: { kind: "linear", start: [0.5, 0], end: [0.5, 0.5] },
            parts: [{ mode: "subtract", shape: { kind: "radial", centre: [0.5, 0.1], radius: [0.1, 0.1], angle: 0, feather: 0 } }],
            adjustments: { exposure: -1, warmth: 0, clarity: 0 },
          },
        ],
      },
      "combined mask frame",
      plainFrame,
    );
    const combined =
      combinedFrame && plainFrame
        ? {
            insideChange:
              Math.round((patchMean(combinedFrame, 0, 0.15, 0.47, 0.53) - patchMean(plainFrame, 0, 0.15, 0.47, 0.53)) * 100) / 100,
            besideDarker: Math.round((patchMean(plainFrame, 0, 0.1, 0, 0.2) - patchMean(combinedFrame, 0, 0.1, 0, 0.2)) * 10) / 10,
            renderMs: combinedFrame.frame.renderMs,
          }
        : null;

    const maskOk =
      linearMaskOk &&
      radial !== null &&
      radial.edgesDarker > 5 &&
      Math.abs(radial.middleChange) < 0.5 &&
      brush !== null &&
      brush.bandDarker > 5 &&
      Math.abs(brush.topChange) < 0.5 &&
      brush.erasedMiddleChange !== null &&
      Math.abs(brush.erasedMiddleChange) < 0.5 &&
      (brush.erasedLeftDarker ?? 0) > 5 &&
      !("error" in uiBrush) &&
      uiBrush.erasedMiddleChange !== null &&
      Math.abs(uiBrush.erasedMiddleChange) < 1 &&
      (uiBrush.paintedLeftDarker ?? 0) > 5 &&
      combined !== null &&
      Math.abs(combined.insideChange) < 0.5 &&
      combined.besideDarker > 5;

    // A quarter turn (ADR 0039): the frame is the photo on its side.
    const framesBeforeTurn = frames.length;
    driver.editor().setRecipe({
      ...beforeCrop,
      geometry: {
        straighten: 0,
        crop: { x: 0, y: 0, w: 1, h: 1 },
        aspect: "original",
        vertical: 0,
        horizontal: 0,
        rotation: 1,
        flip: false,
      },
    });
    const turnedFrame = await waitFor(
      () => frames.slice(framesBeforeTurn).find((f) => f.frame.fullWidth === fullH && !f.frame.cacheHit) ?? null,
      10_000,
      "turned frame",
    ).catch(() => null);
    driver.editor().setRecipe(beforeCrop);
    const turn = {
      fullSize: turnedFrame ? `${turnedFrame.frame.fullWidth}x${turnedFrame.frame.fullHeight}` : null,
      frameSize: turnedFrame ? `${turnedFrame.frame.width}x${turnedFrame.frame.height}` : null,
      renderMs: turnedFrame?.frame.renderMs ?? null,
    };
    const turnOk =
      turnedFrame !== null && turnedFrame.frame.fullHeight === fullW && turnedFrame.frame.height > turnedFrame.frame.width;

    // Perspective (ADR 0034): the renderer fits the same crop as the crop tool.
    const shape = { straighten: 0, vertical: 40, horizontal: -15, rotation: 0, flip: false };
    const fitted = fitCropFor(fullW / fullH, shape, fullW, fullH);
    const framesBeforePerspective = frames.length;
    driver.editor().setRecipe({ ...beforeCrop, geometry: { ...shape, crop: fitted, aspect: "original" } });
    const corrected = await waitFor(
      () =>
        frames
          .slice(framesBeforePerspective)
          .find((f) => Math.abs(f.frame.fullWidth - fitted.w * fullW) <= 2 && !f.frame.cacheHit) ?? null,
      10_000,
      "perspective frame",
    ).catch(() => null);
    driver.editor().setRecipe(beforeCrop);
    const perspective = {
      expected: `${Math.round(fitted.w * fullW)}x${Math.round(fitted.h * fullH)}`,
      fullSize: corrected ? `${corrected.frame.fullWidth}x${corrected.frame.fullHeight}` : null,
      renderMs: corrected?.frame.renderMs ?? null,
    };
    const perspectiveOk = corrected !== null && Math.abs(corrected.frame.fullHeight - fitted.h * fullH) <= 2;

    // Remove chromatic aberration (ADR 0035): measured through the real command, then
    // rendered with (the whole frame, freshly resampled).
    const tCa = performance.now();
    let caError: string | null = null;
    const measured = await ipc.measureChromaticAberration(driver.editor().image!.id).catch((e: unknown) => {
      caError = e instanceof Error ? e.message : JSON.stringify(e);
      return undefined;
    });
    const caMs = Math.round(performance.now() - tCa);
    let caFrame: RenderedFrame | null = null;
    if (measured) {
      const framesBeforeCa = frames.length;
      driver.editor().setRecipe({ ...beforeCrop, chromaticAberration: measured });
      caFrame = await waitFor(
        () =>
          frames
            .slice(framesBeforeCa)
            .find((f) => f.frame.fullWidth === fullW && !f.frame.cacheHit) ?? null,
        10_000,
        "chromatic aberration frame",
      ).catch(() => null);
      driver.editor().setRecipe(beforeCrop);
    }
    const chromaticAberration = {
      measured: measured ?? null,
      ms: caMs,
      error: caError,
      renderMs: caFrame?.frame.renderMs ?? null,
    };
    const chromaticAberrationOk = measured !== undefined && caMs < 3000 && (measured === null || caFrame !== null);
    // Every viewer frame carries its histogram (ADR 0036), counting each pixel once.
    const histogramChecked = frames.map((f) => {
      const h = f.frame.histogram;
      return h !== null && h.luma.reduce((a, b) => a + b, 0) === f.frame.width * f.frame.height;
    });
    const histogram = { frames: frames.length, withHistogram: histogramChecked.filter(Boolean).length };
    const histogramOk = frames.length > 0 && histogramChecked.every(Boolean);

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
      toneCurve: toneCurveOk,
      history: historyOk,
      crop: cropOk,
      perspective: perspectiveOk,
      turn: turnOk,
      mask: maskOk,
      chromaticAberration: chromaticAberrationOk,
      histogram: histogramOk,
      autoLevel: autoLevelOk,
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
      toneCurve,
      history,
      crop,
      perspective,
      turn,
      mask,
      radial,
      brush,
      uiBrush,
      combined,
      chromaticAberration,
      histogram,
      autoLevel,
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
