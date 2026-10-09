import type { WatermarkDto } from "../../ipc/generated/WatermarkDto";
import { watermarkFor } from "../export/watermark";
import * as ipc from "../../ipc/client";
import type { ExportFileFormat } from "../../ipc/generated/ExportFileFormat";
import type { OutputSharpening } from "../../ipc/generated/OutputSharpening";
import type { ExportColourSpace } from "../../ipc/generated/ExportColourSpace";
import type { ExportQueueEvent } from "../../ipc/generated/ExportQueueEvent";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { ExportEvent } from "../../ipc/generated/ExportEvent";
import type { IndexEvent } from "../../ipc/generated/IndexEvent";
import type { SelfTestConfigDto } from "../../ipc/generated/SelfTestConfigDto";
import { fitCropFor } from "../editor/cropGeometry";
import { applyPreset } from "../editor/presets";
import { beforeRecipe, mixerOf, neutralRecipe } from "../editor/recipe";
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

    // The export dialog's size estimate (ADR 0068) for the same photo, edit and
    // settings (the defaults: JPEG 85, full size, Screen, sRGB), against the file made.
    const sizeEstimate = await (async () => {
      const image = driver.editor().image;
      if (!image || finished.type !== "finished") return null;
      const t = performance.now();
      const e = await ipc
        .estimateExport({ imageId: image.id, recipe: a, format: "jpeg", quality: 85, sharpen: "screen", colourSpace: "srgb" })
        .catch(() => null);
      const ms = Math.round(performance.now() - t);
      return e ? { estimate: e.bytes, actual: finished.bytes, size: `${e.width}x${e.height}`, errorPct: Math.round((e.bytes / finished.bytes - 1) * 1000) / 10, ms } : null;
    })();

    // Auto (ADR 0071): the tone sliders found on a small sample, quickly; the photo
    // rendered with them has its median near the middle (unless Exposure is at a
    // limit), and the button is in the preset strip.
    const autoTone = await (async () => {
      const image = driver.editor().image;
      if (!image) return null;
      const t = performance.now();
      const tone = await ipc.autoTone(image.id, a);
      const ms = Math.round(performance.now() - t);
      const frame = await ipc.renderPreview({ imageId: image.id, recipe: { ...a, ...tone }, quality: "detail", targetLongEdge: 1600, slot: "compare" });
      const luma = frame.histogram?.luma;
      let median = null as number | null;
      if (luma) {
        const total = luma.reduce((x, y) => x + y, 0);
        let seen = 0;
        median = luma.findIndex((n) => (seen += n) >= total / 2) / 255;
      }
      const atLimit = tone.exposure <= -2 || tone.exposure >= 2.5;
      // Auto for one setting (Shift-double-click on its slider): aimed at its target,
      // so it moves even where Auto leaves the photo alone. Blacks brings the black end
      // (0.5th percentile) nearer its target; Whites stays put on a photo with nothing
      // near white (it can't change it).
      const exposureAlone = await ipc.autoSetting(image.id, a, "exposure");
      const blacksAlone = await ipc.autoSetting(image.id, a, "blacks");
      const whitesAlone = await ipc.autoSetting(image.id, a, "whites");
      const blackEndOf = async (recipe: typeof a) => {
        const frame = await ipc.renderPreview({ imageId: image.id, recipe, quality: "detail", targetLongEdge: 1600, slot: "compare" });
        const l = frame.histogram?.luma;
        if (!l) return null;
        const total = l.reduce((x, y) => x + y, 0);
        let seen = 0;
        return l.findIndex((n) => (seen += n) >= total * 0.005) / 255;
      };
      const plainEnd = await blackEndOf(a);
      const blackEnd = await blackEndOf({ ...a, blacks: blacksAlone });
      return {
        tone,
        ms,
        median,
        button: document.querySelector("button.auto-tone") !== null,
        exposureAlone,
        blacksAlone,
        whitesAlone,
        blackEnd,
        plainEnd,
        ok:
          Number.isFinite(exposureAlone) &&
          blackEnd !== null &&
          plainEnd !== null &&
          blacksAlone !== 0 &&
          Math.abs(blackEnd - 0.02) <= Math.abs(plainEnd - 0.02) && median !== null && (atLimit || Math.abs(median - 0.43) < 0.1) && ms < 3000 && document.querySelector("button.auto-tone") !== null,
      };
    })();

    // Zoom (ADR 0070): a window at 100 % renders from the preview source until the full
    // resolution is decoded, then pixel for pixel from it; Z in the editor shows it.
    const zoom = await (async () => {
      const image = driver.editor().image;
      if (!image) return null;
      await sleep(300);
      const [fw, fh] = [image.fullWidth, image.fullHeight];
      // Windows at 100 %: the whole photo's long edge at the zoom is its own.
      const win: [number, number, number, number] = [Math.round(fw / 2 - 1000), Math.round(fh / 2 - 600), 2000, 1200];
      // In the comparison's slot: the editor's own renders (such as one after a fill
      // made in the background) must not cancel these, nor these the editor's.
      const request = (quality: "interactive" | "detail", window: [number, number, number, number]) =>
        ipc.renderPreview({ imageId: image.id, recipe: a, quality, targetLongEdge: Math.max(image.fullWidth, image.fullHeight), window, slot: "compare" });
      const early = await request("detail", win);
      const t = performance.now();
      await ipc.prepareFull(image.id);
      const fullDecodeMs = Math.round(performance.now() - t);
      const full = await request("detail", win);
      const panned = await request("interactive", [win[0] + 160, win[1], win[2], win[3]]);
      const fullPanned = await request("detail", [win[0] + 160, win[1], win[2], win[3]]);
      const shown = full.window;
      // At 50 %, from the half-size preview: half the pixels each way, and quicker.
      const half = await ipc.renderPreview({ imageId: image.id, recipe: a, quality: "detail", targetLongEdge: Math.round(Math.max(fw, fh) / 2), window: win, slot: "compare" });
      // A heavier edit: clarity reads around each pixel, and a straightened crop frames
      // the whole full-resolution source first (once per framing; then cached).
      const heavy = {
        ...a,
        clarity: 40,
        texture: 20,
        geometry: { straighten: 2, crop: { x: 0.05, y: 0.05, w: 0.9, h: 0.9 }, aspect: "free" as const, vertical: 0, horizontal: 0, rotation: 0, flip: false },
      };
      const heavyRequest = (window: [number, number, number, number]) =>
        ipc.renderPreview({ imageId: image.id, recipe: heavy, quality: "detail", targetLongEdge: Math.max(image.fullWidth, image.fullHeight), window, slot: "compare" });
      // A brush mask at 100 %: its coverage is rasterised at full size once, then cached.
      const brushed = {
        ...a,
        masks: [
          {
            id: 1,
            shape: { kind: "brush" as const, strokes: [{ size: 0.01, feather: 40, flow: 100, points: [[0.3, 0.5], [0.5, 0.52], [0.7, 0.5]] as [number, number][] }] },
            adjustments: { exposure: -1, warmth: 0, clarity: 0 },
          },
        ],
      };
      const brushRequest = (window: [number, number, number, number]) =>
        ipc.renderPreview({ imageId: image.id, recipe: brushed, quality: "detail", targetLongEdge: Math.max(image.fullWidth, image.fullHeight), window, slot: "compare" });
      const brushFirst = await brushRequest(win);
      const brushPanned = await brushRequest([win[0] + 160, win[1], win[2], win[3]]);
      const heavyFirst = await heavyRequest([win[0], win[1] - 200, win[2], win[3]]);
      const heavyPanned = await heavyRequest([win[0] + 160, win[1] - 200, win[2], win[3]]);
      // The editor: Z zooms to 100 % and its window arrives at full resolution; Z again fits.
      const press = () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", bubbles: true }));
      press();
      const ui = await waitFor(() => {
        const w = driver.editor().windowed?.frame;
        return w?.window && w.width === Math.round(w.window.width) ? w : null;
      }, 10_000, "zoomed window").catch(() => null);
      press();
      // Back to Fit, animated.
      await sleep(700);
      return {
        early: `${early.width}x${early.height}`,
        fullDecodeMs,
        full: `${full.width}x${full.height}`,
        window: shown ? [shown.x, shown.y, shown.width, shown.height] : null,
        detailRenderMs: Math.round(full.renderMs),
        interactiveRenderMs: Math.round(panned.renderMs),
        interactive: `${panned.width}x${panned.height}`,
        pannedDetailRenderMs: Math.round(fullPanned.renderMs),
        half: `${half.width}x${half.height}`,
        halfRenderMs: Math.round(half.renderMs),
        heavyFirstRenderMs: Math.round(heavyFirst.renderMs),
        brushFirstRenderMs: Math.round(brushFirst.renderMs),
        brushPannedRenderMs: Math.round(brushPanned.renderMs),
        heavyPannedRenderMs: Math.round(heavyPanned.renderMs),
        ui: ui ? `${ui.width}x${ui.height}` : null,
        uiFit: driver.editor().windowed === null,
        ok:
          early.width < win[2] &&
          full.width === win[2] &&
          full.height === win[3] &&
          shown !== null &&
          shown.x === win[0] &&
          shown.y === win[1] &&
          panned.width < win[2] &&
          fullPanned.width === win[2] &&
          half.width === win[2] / 2 &&
          heavyPanned.width === win[2] &&
          ui !== null &&
          driver.editor().windowed === null,
      };
    })();

    // Focus mode (ADR 0072): the top bar's button hides the adjustments panel and the
    // photo takes the room; F brings the panel back.
    const focusMode = await (async () => {
      const button = document.querySelector<HTMLButtonElement>("button.focus-button");
      const panel = () => document.querySelector<HTMLElement>(".panel-right");
      const width = () => document.querySelector(".viewer")?.getBoundingClientRect().width ?? 0;
      if (!button || !panel()) return { ok: false };
      const before = width();
      button.click();
      await sleep(300);
      const hidden = panel()!.hidden && getComputedStyle(panel()!).display === "none";
      const wider = width() > before + 100;
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", bubbles: true }));
      await sleep(300);
      const back = !panel()!.hidden && Math.abs(width() - before) < 2;
      return { hidden, wider, back, ok: hidden && wider && back };
    })();

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
      await ipc.setPhotoMarks([target], { type: "label", label: "purple" });
      const counts = await ipc.setPhotoMarks([target], { type: "flag", flag: "pick" });
      const listed = (await ipc.listFolder(indexing.folder)).photos.find((p) => p.path === target)?.marks ?? null;
      const picks = await ipc.libraryCollection("picks");
      await ipc.setPhotoMarks([target], { type: "rating", stars: 0 });
      const cleared = await ipc.setPhotoMarks([target], { type: "flag", flag: "none" });
      // A label alone still lists (ADR 0064), then is cleared too.
      const labelOnly = (await ipc.listFolder(indexing.folder)).photos.find((p) => p.path === target)?.marks ?? null;
      await ipc.setPhotoMarks([target], { type: "label", label: "none" });
      const unlabelled = (await ipc.listFolder(indexing.folder)).photos.find((p) => p.path === target)?.marks ?? null;
      return {
        counts,
        listed,
        inPicks: picks.photos.some((p) => p.path === target),
        pickLabel: picks.photos.find((p) => p.path === target)?.marks.label ?? null,
        cleared,
        labelOnly,
        unlabelled,
      };
    })();
    const marksOk =
      marks !== null &&
      marks.listed?.rating === 4 &&
      marks.listed.flag === "pick" &&
      marks.inPicks &&
      marks.counts.picks >= 1 &&
      marks.cleared.picks === marks.counts.picks - 1;
    const labelsOk =
      marks !== null &&
      marks.listed?.label === "purple" &&
      marks.pickLabel === "purple" &&
      marks.labelOnly?.label === "purple" &&
      marks.labelOnly.rating === 0 &&
      marks.labelOnly.flag === "none" &&
      marks.unlabelled?.label === "none";

    // All photos (ADR 0065): every present photo, as many as the sidebar counts, the
    // test folder's photos among them. Then the Library's view choices round-trip
    // through the (temporary) settings file.
    const allPhotos = await (async () => {
      if (!indexing) return null;
      const t = performance.now();
      const all = await ipc.libraryCollection("all");
      const ms = Math.round((performance.now() - t) * 10) / 10;
      const status = await ipc.libraryStatus();
      const folder = (await ipc.listFolder(indexing.folder)).photos.map((p) => p.path);
      const listed = new Set(all.photos.map((p) => p.path));
      const before = (await ipc.getSettings()).settings;
      const chosen = { layout: "list", filter: "picks", label: "blue", sort: "newest" } as const;
      const stored = (await ipc.updateSettings({ ...before, library: { ...before.library, view: chosen } })).settings.library.view;
      await ipc.updateSettings(before);
      // Where the Library was left: recorded by Rust (a folder only when granted, by
      // its canonical path) and never by a settings update.
      const placeOf = async () => (await ipc.getSettings()).settings.library.lastPlace;
      await ipc.rememberPlace({ kind: "collection", collection: "all" });
      const collectionPlace = await placeOf();
      await ipc.rememberPlace({ kind: "folder", path: `${indexing.folder}/.` });
      const folderPlace = await placeOf();
      const outside = await ipc.rememberPlace({ kind: "folder", path: "/" }).then(
        () => "stored",
        () => "refused",
      );
      const afterOutside = await placeOf();
      const current = (await ipc.getSettings()).settings;
      await ipc.updateSettings({ ...current, library: { ...current.library, lastPlace: { kind: "album", id: 999 } } });
      const afterUpdate = await placeOf();
      return {
        photos: all.photos.length,
        count: status.collections.all,
        folderPhotosListed: folder.filter((p) => listed.has(p)).length,
        folderPhotos: folder.length,
        ms,
        viewStored: stored,
        place: { collectionPlace, folderPlace, outside, afterOutside, afterUpdate },
      };
    })();
    const allPhotosOk =
      allPhotos !== null &&
      allPhotos.photos > 0 &&
      allPhotos.photos === allPhotos.count &&
      allPhotos.folderPhotosListed === allPhotos.folderPhotos &&
      allPhotos.viewStored.layout === "list" &&
      allPhotos.viewStored.filter === "picks" &&
      allPhotos.viewStored.label === "blue" &&
      allPhotos.viewStored.sort === "newest";
    const lastPlaceOk =
      allPhotos !== null &&
      allPhotos.place.collectionPlace?.kind === "collection" &&
      allPhotos.place.collectionPlace.collection === "all" &&
      allPhotos.place.folderPlace?.kind === "folder" &&
      // Stored canonical: the "/." is gone.
      allPhotos.place.folderPlace.path === indexing?.folder &&
      allPhotos.place.outside === "refused" &&
      JSON.stringify(allPhotos.place.afterOutside) === JSON.stringify(allPhotos.place.folderPlace) &&
      JSON.stringify(allPhotos.place.afterUpdate) === JSON.stringify(allPhotos.place.folderPlace);

    // Albums (ADR 0055) through the real commands and catalogue: make one with two
    // photos, add a third (and one again), list it, take one out, rename and delete it.
    const albums = await (async () => {
      if (!indexing) return null;
      const paths = (await ipc.listFolder(indexing.folder)).photos.map((p) => p.path).slice(0, 3);
      if (paths.length < 3) return null;
      const made = await ipc.createAlbum("  Self-test   album ", paths.slice(0, 2));
      const added = await ipc.addToAlbum(made.id, [paths[2]!, paths[0]!]);
      const listed = await ipc.albumPhotos(made.id);
      const removed = await ipc.removeFromAlbum(made.id, [paths[1]!]);
      const renamed = await ipc.renameAlbum(made.id, "Renamed");
      const before = (await ipc.listAlbums()).some((a) => a.id === made.id);
      await ipc.deleteAlbum(made.id);
      const after = (await ipc.listAlbums()).some((a) => a.id === made.id);
      const stillIndexed = (await ipc.listFolder(indexing.folder)).photos.filter((p) => paths.includes(p.path)).length;
      return {
        name: made.name,
        made: made.count,
        added: added.count,
        listed: listed.photos.length,
        cover: listed.album.cover !== null,
        removed: removed.count,
        renamed: renamed.name,
        listedBefore: before,
        listedAfterDelete: after,
        photosKept: stillIndexed,
      };
    })();
    const albumsOk =
      albums !== null &&
      albums.name === "Self-test album" &&
      albums.made === 2 &&
      albums.added === 3 &&
      albums.listed === 3 &&
      albums.cover &&
      albums.removed === 2 &&
      albums.renamed === "Renamed" &&
      albums.listedBefore &&
      !albums.listedAfterDelete &&
      albums.photosKept === 3;

    // Search and Recently imported (ADR 0056) through the real commands and catalogue:
    // the fixtures folder's name, the test photo's camera and capture year find it; a
    // nonsense word finds nothing; the photos just indexed are recently imported.
    const searchCheck = await (async () => {
      if (!indexing) return null;
      const folderName = indexing.folder.split(/[\\/]/).filter(Boolean).pop() ?? "";
      const target = (await ipc.listFolder(indexing.folder)).photos.find((p) => p.path === config.imagePath);
      const camera = target?.details?.camera?.split(/\s+/)[0] ?? "";
      const year = target?.details?.capturedAt?.slice(0, 4) ?? "";
      const t = performance.now();
      const byFolder = await ipc.searchLibrary(folderName);
      const ms = Math.round((performance.now() - t) * 10) / 10;
      const byCameraYear = await ipc.searchLibrary(`${camera} ${year}`);
      const nonsense = await ipc.searchLibrary("zzqxv-nothing");
      const recent = await ipc.libraryCollection("recent");
      const status = await ipc.libraryStatus();
      const has = (r: { photos: { path: string }[] }) => r.photos.some((p) => p.path === config.imagePath);
      return {
        query: `${folderName} | ${camera} ${year}`,
        byFolder: byFolder.photos.length,
        foundByFolder: has(byFolder),
        foundByCameraYear: has(byCameraYear),
        nonsense: nonsense.photos.length,
        recent: recent.photos.length,
        recentCount: status.collections.recent,
        recentHasPhoto: has(recent),
        searchMs: ms,
      };
    })();
    const searchOk =
      searchCheck !== null &&
      searchCheck.foundByFolder &&
      searchCheck.foundByCameraYear &&
      searchCheck.nonsense === 0 &&
      searchCheck.recentHasPhoto &&
      searchCheck.recent === searchCheck.recentCount;

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

    // Batch edits (ADR 0049) through the real command and catalogue: sync a look onto
    // the folder's other photos. They are saved as edited; one with its own crop keeps
    // it (the crop is not synced by default); a photo outside the library is refused.
    const batch = await (async () => {
      if (!indexing) return null;
      const others = (await ipc.listFolder(indexing.folder)).photos.map((p) => p.path).filter((p) => p !== config.imagePath);
      if (others.length < 2) return null;
      const neutral = neutralRecipe(info.recipeVersion);
      const ownCrop = { x: 0.2, y: 0.1, w: 0.6, h: 0.8 };
      const cropped = { ...neutral, geometry: { straighten: 0, crop: ownCrop, aspect: "free" as const, vertical: 0, horizontal: 0, rotation: 0, flip: false } };
      await ipc.saveEdit(others[0]!, cropped);
      const source = { ...neutral, contrast: 40, saturation: -30, geometry: { ...cropped.geometry, crop: { x: 0, y: 0, w: 0.5, h: 0.5 } } };
      const groups = info.settingGroups.filter((g) => g.copiedByDefault).map((g) => g.id);
      const t0 = performance.now();
      const result = await ipc.pasteEditsTo([...others, "/not/in/the/library.jpg"], source, groups);
      const ms = Math.round(performance.now() - t0);
      const listed = (await ipc.listFolder(indexing.folder)).photos.filter((p) => others.includes(p.path));
      const reopened = await ipc.openImagePath(others[0]!);
      for (const p of others) await ipc.saveEdit(p, neutral);
      return {
        photos: others.length,
        ms,
        applied: result.applied.length,
        failed: result.failed.map((f) => f.message),
        allEdited: listed.length === others.length && listed.every((p) => p.edited),
        syncedLook: reopened?.savedRecipe?.contrast === 40 && reopened.savedRecipe.saturation === -30,
        keptOwnCrop: JSON.stringify(reopened?.savedRecipe?.geometry?.crop) === JSON.stringify(ownCrop),
      };
    })();
    const batchOk =
      batch !== null &&
      batch.applied === batch.photos &&
      batch.failed.length === 1 &&
      batch.allEdited &&
      batch.syncedLook &&
      batch.keptOwnCrop;

    // The export queue (ADR 0050) through the real command, engine and files: three
    // photos at 1350 px into the temporary folder, one after another, reporting
    // progress; then a full-size run of all of them, cancelled after it starts.
    const exportQueue = await (async () => {
      if (!indexing) return null;
      const folder = config.exportPath.replace(/[^/\\]+$/, "");
      const photos = (await ipc.listFolder(indexing.folder)).photos.map((p) => p.path);
      const run = async (
        items: string[],
        longEdge: number | undefined,
        cancelAfterStart: boolean,
        format: ExportFileFormat = "jpeg",
        sharpen: OutputSharpening = "screen",
        colourSpace: ExportColourSpace = "srgb",
        metadata: { keepMetadata?: boolean; stripLocation?: boolean; watermark?: WatermarkDto } = {},
      ) => {
        let progress = 0;
        let finished: Extract<ExportQueueEvent, { type: "finished" }> | null = null;
        const unlisten = await ipc.onExportQueueEvent((e) => {
          if (e.type === "progress") {
            progress++;
            if (cancelAfterStart && progress === 1) void ipc.cancelExports();
          } else finished = e;
        });
        const t0 = performance.now();
        await ipc.startExport({ items: items.map((path) => ({ path })), longEdge, quality: 85, format, sharpen, colourSpace, ...metadata, folder });
        const done = await waitFor(() => finished, 120_000, "export queue").catch(() => null);
        unlisten();
        return { done, progress, ms: Math.round(performance.now() - t0) };
      };
      const sized = await run(photos.slice(0, 3), 1350, false);
      const stopped = await run(photos, undefined, true);
      // The other formats (ADR 0057): the test photo as a 16-bit TIFF and a PNG.
      const output = async (format: ExportFileFormat) => {
        const t = performance.now();
        const r = await run([config.imagePath], 1350, false, format);
        const o = r.done?.outputs[0];
        return o ? { file: o.path.split(/[\\/]/).pop(), size: `${o.width}x${o.height}`, kb: Math.round(o.bytes / 1024), ms: Math.round(performance.now() - t) } : null;
      };
      const formats = { jpeg: sized.done?.outputs[0]?.bytes ?? null, tiff: await output("tiff"), png: await output("png") };
      // Output sharpening (ADR 0059): sharpened for matte paper, the same JPEG holds
      // more fine detail, so it is larger than unsharpened.
      const sharpenedKb = async (sharpen: OutputSharpening) =>
        Math.round(((await run([config.imagePath], 1350, false, "jpeg", sharpen)).done?.outputs[0]?.bytes ?? 0) / 1024);
      const sharpening = { noneKb: await sharpenedKb("none"), screenKb: await sharpenedKb("screen"), matteKb: await sharpenedKb("matte") };
      // Colour spaces (ADR 0062): the same JPEG in each space (converted and tagged,
      // so a different file), and Full quality's 16-bit Adobe RGB TIFF.
      const inSpace = async (colourSpace: ExportColourSpace, format: ExportFileFormat = "jpeg") => {
        const r = await run([config.imagePath], 1350, false, format, "screen", colourSpace);
        const o = r.done?.outputs[0];
        return o ? { bytes: o.bytes, ms: r.ms } : null;
      };
      const colourSpaces = {
        srgb: await inSpace("srgb"),
        displayP3: await inSpace("displayP3"),
        adobeRgb: await inSpace("adobeRgb"),
        adobeRgbTiff: await inSpace("adobeRgb", "tiff"),
      };
      // Metadata (ADR 0063): the same JPEG with everything, without the location, and
      // with nothing. The pixels are identical, so only the EXIF changes the size.
      const withMetadata = async (keepMetadata: boolean, stripLocation: boolean) =>
        (await run([config.imagePath], 1350, false, "jpeg", "screen", "srgb", { keepMetadata, stripLocation })).done?.outputs[0]?.bytes ?? null;
      // The photographer's marks (ADR 0067) go with the metadata as XMP: rated and
      // labelled, the same export grows by the packet; with metadata off, it doesn't.
      await ipc.setPhotoMarks([config.imagePath], { type: "rating", stars: 4 });
      await ipc.setPhotoMarks([config.imagePath], { type: "label", label: "purple" });
      const marked = await withMetadata(true, false);
      const markedNone = await withMetadata(false, false);
      await ipc.setPhotoMarks([config.imagePath], { type: "rating", stars: 0 });
      await ipc.setPhotoMarks([config.imagePath], { type: "label", label: "none" });
      // A watermark (ADR 0069), drawn by the UI as the dialog's export does, laid on
      // by the queue: the same JPEG, plus the text in its corner.
      const drawn = await watermarkFor({ enabled: true, text: "© 2026 Self-test", position: "bottomRight", size: "large" });
      const watermarked = drawn
        ? (await run([config.imagePath], 1350, false, "jpeg", "screen", "srgb", { keepMetadata: false, watermark: drawn })).done?.outputs[0] ?? null
        : null;
      // Repeated across the photo: far more text, so a far larger file.
      const repeatedDrawn = drawn ? { ...drawn, position: "repeat" as const, size: "medium" as const } : null;
      const repeated = repeatedDrawn
        ? (await run([config.imagePath], 1350, false, "jpeg", "screen", "srgb", { keepMetadata: false, watermark: repeatedDrawn })).done?.outputs[0] ?? null
        : null;
      const watermark = {
        drawnBytes: drawn?.png.length ?? null,
        file: watermarked?.path ?? null,
        bytes: watermarked?.bytes ?? null,
        repeatedFile: repeated?.path ?? null,
        repeatedBytes: repeated?.bytes ?? null,
      };
      const metadata = {
        all: await withMetadata(true, false),
        withoutLocation: await withMetadata(true, true),
        none: await withMetadata(false, false),
        marked,
        markedNone,
      };
      return {
        formats,
        sharpening,
        colourSpaces,
        metadata,
        watermark,
        exported: sized.done?.exported ?? null,
        longEdges: sized.done?.outputs.map((o) => Math.max(o.width, o.height)) ?? [],
        failed: sized.done?.failed.length ?? null,
        progressEvents: sized.progress,
        msPerPhoto: sized.done ? Math.round(sized.ms / 3) : null,
        cancelled: stopped.done?.cancelled ?? null,
        exportedBeforeCancel: stopped.done?.exported ?? null,
        of: photos.length,
      };
    })();
    const formatsOk =
      exportQueue !== null &&
      exportQueue.formats.tiff?.file?.endsWith(".tif") === true &&
      exportQueue.formats.png?.file?.endsWith(".png") === true &&
      [exportQueue.formats.tiff, exportQueue.formats.png].every((o) => o !== null && Math.max(...o.size.split("x").map(Number)) === 1350) &&
      // A 16-bit TIFF is far larger than the PNG, which is larger than a JPEG.
      exportQueue.formats.tiff!.kb > exportQueue.formats.png!.kb &&
      exportQueue.formats.png!.kb * 1024 > (exportQueue.formats.jpeg ?? Infinity);
    const sharpeningOk =
      exportQueue !== null &&
      exportQueue.sharpening.noneKb > 0 &&
      exportQueue.sharpening.screenKb > exportQueue.sharpening.noneKb &&
      exportQueue.sharpening.matteKb > exportQueue.sharpening.screenKb;
    const colourSpacesOk =
      exportQueue !== null &&
      Object.values(exportQueue.colourSpaces).every((o) => o !== null && o.bytes > 0) &&
      new Set([exportQueue.colourSpaces.srgb, exportQueue.colourSpaces.displayP3, exportQueue.colourSpaces.adobeRgb].map((o) => o?.bytes)).size === 3;
    // EXIF is at least a few hundred bytes; the location (when the photo has one) more.
    const metadataOk =
      exportQueue !== null &&
      exportQueue.metadata.none !== null &&
      exportQueue.metadata.withoutLocation !== null &&
      exportQueue.metadata.all !== null &&
      exportQueue.metadata.withoutLocation - exportQueue.metadata.none > 200 &&
      exportQueue.metadata.all >= exportQueue.metadata.withoutLocation;
    // An XMP packet with a rating and a label is about 350 bytes.
    // The text adds detail to the corner: the file grows over the plain one (without
    // metadata either).
    const watermarkOk =
      exportQueue !== null &&
      exportQueue.watermark.drawnBytes !== null &&
      exportQueue.watermark.bytes !== null &&
      exportQueue.metadata.none !== null &&
      exportQueue.watermark.bytes > exportQueue.metadata.none + 500 &&
      exportQueue.watermark.repeatedBytes !== null &&
      exportQueue.watermark.repeatedBytes > exportQueue.watermark.bytes;
    const marksInExportOk =
      exportQueue !== null &&
      exportQueue.metadata.marked !== null &&
      exportQueue.metadata.all !== null &&
      exportQueue.metadata.marked - exportQueue.metadata.all > 250 &&
      exportQueue.metadata.markedNone === exportQueue.metadata.none;
    const exportQueueOk =
      exportQueue !== null &&
      exportQueue.exported === 3 &&
      exportQueue.longEdges.every((e) => e === 1350) &&
      exportQueue.failed === 0 &&
      exportQueue.progressEvents > 3 &&
      exportQueue.cancelled === true &&
      exportQueue.exportedBeforeCancel !== null &&
      exportQueue.exportedBeforeCancel < exportQueue.of;

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

    // Before/after (ADR 0045) through the Compare button: the before image is the
    // photo unedited (as a plain render of it shows), editing while comparing still
    // renders the edit, and does not render the before image again.
    const compareCheck = await (async () => {
      const rgbMean = (pixels: Uint8ClampedArray) => {
        let sum = 0;
        for (let i = 0; i < pixels.length; i += 4) sum += pixels[i]! + pixels[i + 1]! + pixels[i + 2]!;
        return sum / ((pixels.length / 4) * 3);
      };
      const round = (v: number) => Math.round(v * 10) / 10;
      const edited = { ...beforeCrop, masks: undefined, exposure: beforeCrop.exposure + 1 };
      await show(edited, "edit to compare");
      const button = () =>
        [...document.querySelectorAll<HTMLElement>(".photo-toolbar button")].find((b) => b.textContent?.trim() === "Compare");
      await waitFor(() => button() ?? null, 5_000, "Compare button");
      const t0 = performance.now();
      button()!.click();
      const canvas = await waitFor(
        () => {
          const c = document.querySelector<HTMLCanvasElement>(".compare-before");
          return c?.dataset.drawn ? c : null;
        },
        10_000,
        "before image",
      ).catch(() => null);
      const beforeMs = Math.round(performance.now() - t0);
      if (!canvas) return { error: "no before image" as const };
      const drawn = canvas.dataset.drawn;
      const readBefore = () => canvas.getContext("2d")!.getImageData(0, 0, canvas.width, canvas.height).data;
      const beforeMean = rgbMean(readBefore());
      const live = await show({ ...edited, exposure: edited.exposure + 0.5 }, "edit while comparing");
      await nextFrame();
      const redrawn = canvas.dataset.drawn !== drawn;
      button()!.click();
      await nextFrame();
      const closed = document.querySelector(".compare-before") === null;
      const reference = await show(beforeRecipe(edited, info.adjustments), "unedited frame");
      return {
        beforeMs,
        size: `${canvas.width}x${canvas.height}`,
        beforeMean: round(beforeMean),
        uneditedMean: reference ? round(rgbMean(reference.frame.pixels)) : null,
        editMean: live ? round(rgbMean(live.frame.pixels)) : null,
        liveEditRendered: live !== null,
        beforeRenderedAgain: redrawn,
        closed,
      };
    })();
    const compareOk =
      !("error" in compareCheck) &&
      compareCheck.uneditedMean !== null &&
      compareCheck.editMean !== null &&
      Math.abs(compareCheck.beforeMean - compareCheck.uneditedMean) < 2 &&
      compareCheck.editMean > compareCheck.beforeMean + 20 &&
      compareCheck.liveEditRendered &&
      !compareCheck.beforeRenderedAgain &&
      compareCheck.closed;

    // Presets (ADR 0046): the strip's previews render, clicking Mono through the UI
    // makes the photo black and white and keeps its exposure, as one undo step; saved
    // presets are created, renamed and deleted (the self-test's catalogue is in memory).
    const presetCheck = await (async () => {
      const ed = () => driver.editor();
      const edited = { ...beforeCrop, masks: undefined, exposure: beforeCrop.exposure + 0.5, contrast: 40 };
      const shown = await show(edited, "edit before a preset");
      const t0 = performance.now();
      const tiles = () => [...document.querySelectorAll<HTMLElement>(".preset-apply")];
      const drawn = await waitFor(
        () => (tiles().length >= 6 && tiles().every((t) => !t.querySelector("canvas.empty")) ? tiles().length : null),
        10_000,
        "preset previews",
      ).catch(() => null);
      const previewsMs = Math.round(performance.now() - t0);
      // Each preview's render time from request to frame (the strip renders them one
      // after another, so these add up), for a photo the cache has not seen.
      const previewMs: number[] = [];
      for (const p of await ipc.listPresets()) {
        const t = performance.now();
        await ed().renderPresetPreview(applyPreset({ ...edited, exposure: edited.exposure + 0.01 }, p));
        previewMs.push(Math.round((performance.now() - t) * 10) / 10);
      }
      const mono = tiles().find((t) => t.textContent === "Mono");
      const mark = ed().schedulerStats().requested;
      mono?.click();
      const frame = await waitFor(
        () => frames.find((f) => f.info.seq > mark && shown !== null && f.frame.width === shown.frame.width) ?? null,
        10_000,
        "Mono frame",
      ).catch(() => null);
      await nextFrame();
      const applied = ed().recipe!;
      // Mean channel difference: 0 for a black and white picture.
      let spread = 0;
      if (frame) {
        const px = frame.frame.pixels;
        for (let i = 0; i < px.length; i += 4) spread += Math.abs(px[i]! - px[i + 1]!) + Math.abs(px[i + 1]! - px[i + 2]!);
        spread /= px.length / 4;
      }
      const colourSpread = shown
        ? (() => {
            let sum = 0;
            const px = shown.frame.pixels;
            for (let i = 0; i < px.length; i += 4) sum += Math.abs(px[i]! - px[i + 1]!) + Math.abs(px[i + 1]! - px[i + 2]!);
            return sum / (px.length / 4);
          })()
        : null;
      const undoLabel = ed().undoLabel;
      const saved = await ipc.createPreset("Self-test look", edited);
      await ipc.renamePreset(saved.id, "Self-test renamed");
      const listed = await ipc.listPresets();
      // Preset files (ADR 0047): out to a file and back, a Lightroom preset (the repo's
      // fixture), and a missing file, which fails on its own.
      const dir = config.exportPath.replace(/[^/\\]+$/, "");
      const presetFile = `${dir}photo-editor-self-test.preset`;
      const lightroomFile = config.imagePath.replace(/[/\\]local[/\\][^/\\]+$/, "/presets/lightroom-sample.xmp");
      const written = await ipc.exportPreset(saved.id, presetFile);
      const lrtemplateFile = lightroomFile.replace(/\.xmp$/, ".lrtemplate");
      const imports = await ipc.importPresets([presetFile, lightroomFile, lrtemplateFile, `${dir}missing.xmp`]);
      const roundTrip = imports.imported.find((i) => !i.fromLightroom);
      const fromLightroom = imports.imported.find((i) => i.fromLightroom && i.preset.name === "Soft & Warm");
      const fromLrtemplate = imports.imported.find((i) => i.fromLightroom && i.preset.name !== "Soft & Warm");
      for (const i of imports.imported) await ipc.deletePreset(i.preset.id);
      await ipc.deletePreset(saved.id);
      const after = await ipc.listPresets();
      return {
        previews: drawn,
        previewsMs,
        previewMs,
        pressed: mono?.getAttribute("aria-pressed") ?? null,
        undoLabel,
        exposureKept: applied.exposure === edited.exposure,
        saturation: applied.saturation,
        channelSpread: { before: colourSpread === null ? null : Math.round(colourSpread * 10) / 10, mono: Math.round(spread * 100) / 100 },
        savedLookOnly: saved.recipe.exposure === 0 && saved.recipe.contrast === 40,
        renamed: listed.some((p) => p.id === saved.id && p.name === "Self-test renamed"),
        deleted: !after.some((p) => p.id === saved.id) && after.length === listed.length - 1,
        files: {
          written: written === presetFile,
          roundTrip:
            roundTrip !== undefined &&
            roundTrip.preset.name === "Self-test renamed" &&
            JSON.stringify(roundTrip.preset.recipe) === JSON.stringify(listed.find((p) => p.id === saved.id)?.recipe),
          lightroom: fromLightroom
            ? { name: fromLightroom.preset.name, contrast: fromLightroom.preset.recipe.contrast, leftOut: fromLightroom.leftOut }
            : null,
          lrtemplate: fromLrtemplate
            ? { name: fromLrtemplate.preset.name, kelvin: fromLrtemplate.preset.recipe.whiteBalance?.kelvin ?? null }
            : null,
          failed: imports.failed.map((f) => f.file),
        },
      };
    })();
    const presetOk =
      presetCheck.previews !== null &&
      presetCheck.pressed === "true" &&
      presetCheck.undoLabel === "Mono" &&
      presetCheck.exposureKept &&
      presetCheck.saturation === -100 &&
      presetCheck.channelSpread.before !== null &&
      presetCheck.channelSpread.before > 5 &&
      presetCheck.channelSpread.mono < 1 &&
      presetCheck.savedLookOnly &&
      presetCheck.renamed &&
      presetCheck.deleted &&
      presetCheck.files.written &&
      presetCheck.files.roundTrip &&
      presetCheck.files.lightroom?.name === "Soft & Warm" &&
      presetCheck.files.lightroom.contrast === 18 &&
      presetCheck.files.lightroom.leftOut.join() === "Masks and healing" &&
      presetCheck.files.failed.join() === "missing.xmp" &&
      presetCheck.files.lrtemplate?.name === 'Faded "Film"' &&
      presetCheck.files.lrtemplate.kelvin === 5200;

    // Copy and paste (ADR 0048) through the panel footer and ⇧⌘V: the look comes
    // across, the crop does not (left out by default), as one undo step.
    const copyPasteCheck = await (async () => {
      const ed = () => driver.editor();
      const footer = (label: string) =>
        [...document.querySelectorAll<HTMLButtonElement>(".panel-footer button")].find((b) => b.textContent?.trim() === label);
      const cropped = { x: 0.1, y: 0.1, w: 0.8, h: 0.8 };
      const source = {
        ...beforeCrop,
        masks: undefined,
        contrast: 35,
        temperature: 20,
        clarity: 25,
        geometry: { ...(beforeCrop.geometry ?? { straighten: 0, aspect: "original" as const, vertical: 0, horizontal: 0, rotation: 0, flip: false }), crop: cropped },
      };
      await show(source, "edit to copy");
      await waitFor(() => (ed().recipe === source ? true : null), 5_000, "edit to copy shown");
      footer("Copy")?.click();
      await nextFrame();
      const toastAfterCopy = document.querySelector(".toast")?.textContent ?? null;
      const target = { ...beforeCrop, masks: undefined, geometry: undefined, exposure: beforeCrop.exposure - 0.3 };
      await show(target, "photo to paste onto");
      await waitFor(() => (ed().recipe === target ? true : null), 5_000, "paste target shown");
      const mark = ed().schedulerStats().requested;
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "V", shiftKey: true, metaKey: true, ctrlKey: true }));
      const frame = await waitFor(() => frames.find((f) => f.info.seq > mark) ?? null, 10_000, "pasted frame").catch(() => null);
      await nextFrame();
      const pasted = ed().recipe!;
      return {
        toastAfterCopy,
        toastAfterPaste: document.querySelector(".toast")?.textContent ?? null,
        rendered: frame !== null,
        lookPasted: pasted.contrast === 35 && pasted.temperature === 20 && pasted.clarity === 25,
        exposurePasted: pasted.exposure === source.exposure,
        cropLeftOut: pasted.geometry === undefined,
        undoLabel: ed().undoLabel,
      };
    })();
    driver.editor().setRecipe(beforeCrop);
    const copyPasteOk =
      copyPasteCheck.toastAfterCopy === "Edits copied" &&
      copyPasteCheck.toastAfterPaste === "Pasted to 1 photo" &&
      copyPasteCheck.rendered &&
      copyPasteCheck.lookPasted &&
      copyPasteCheck.exposurePasted &&
      copyPasteCheck.cropLeftOut &&
      copyPasteCheck.undoLabel === "Paste";

    // White balance set as a light (ADR 0051), on the real raw file: the photo's own
    // as-shot light changes nothing; a warmer light warms it (more red than blue).
    const lightCheck = await (async () => {
      const scale = driver.editor().image?.temperatureScale;
      if (!scale) return null;
      const base = { ...beforeCrop, masks: undefined, temperature: 0, tint: 0 };
      const plain = await show(base, "frame with relative white balance");
      const same = await show({ ...base, whiteBalance: { kelvin: scale.asShotKelvin, tint: scale.asShotTint } }, "as-shot light", plain);
      const warmer = await show({ ...base, whiteBalance: { kelvin: scale.asShotKelvin + 2000, tint: scale.asShotTint } }, "warmer light", plain);
      const warmth = (f: RenderedFrame | null) => (f ? meanOf(f, "red") - meanOf(f, "green") : null);
      return {
        asShotKelvin: Math.round(scale.asShotKelvin),
        sameChange: plain && same ? Math.round((meanLuma(same) - meanLuma(plain)) * 100) / 100 : null,
        warmerRedLift: warmth(warmer) !== null && warmth(plain) !== null ? Math.round((warmth(warmer)! - warmth(plain)!) * 10) / 10 : null,
      };
    })();
    driver.editor().setRecipe(beforeCrop);
    const lightOk =
      lightCheck !== null &&
      lightCheck.sameChange !== null &&
      Math.abs(lightCheck.sameChange) < 0.5 &&
      lightCheck.warmerRedLift !== null &&
      lightCheck.warmerRedLift > 3;

    // Colour grading (ADR 0052) on the real raw file: black and white stays grey
    // until split-toned; toned, its shadows and highlights carry colour.
    const gradingCheck = await (async () => {
      const mono = { ...beforeCrop, masks: undefined, saturation: -100 };
      const plain = await show(mono, "black and white frame");
      const toned = await show(
        {
          ...mono,
          colourGrading: {
            shadows: { hue: 210, saturation: 40, luminance: 0 },
            midtones: { hue: 0, saturation: 0, luminance: 0 },
            highlights: { hue: 35, saturation: 40, luminance: 0 },
            global: { hue: 0, saturation: 0, luminance: 0 },
            blending: 50,
            balance: 0,
          },
        },
        "split-toned frame",
        plain,
      );
      // Mean channel spread: 0 for grey.
      const spread = (f: RenderedFrame | null) => {
        if (!f) return null;
        const px = f.frame.pixels;
        let sum = 0;
        for (let i = 0; i < px.length; i += 4) sum += Math.abs(px[i]! - px[i + 2]!);
        return Math.round((sum / (px.length / 4)) * 10) / 10;
      };
      return { greySpread: spread(plain), tonedSpread: spread(toned), renderMs: toned?.frame.renderMs ?? null };
    })();
    driver.editor().setRecipe(beforeCrop);
    const gradingOk =
      gradingCheck.greySpread !== null &&
      gradingCheck.tonedSpread !== null &&
      gradingCheck.greySpread < 1 &&
      gradingCheck.tonedSpread > 5;

    // Calibration (ADR 0053) on the real raw file: the primaries move colours but not
    // brightness; Shadow Tint turns the shadows magenta. (It runs before Saturation,
    // as in Lightroom, so black and white takes it out again.)
    const calibrationCheck = await (async () => {
      const neutral = { shadowTint: 0, redHue: 0, redSaturation: 0, greenHue: 0, greenSaturation: 0, blueHue: 0, blueSaturation: 0 };
      const colour = { ...beforeCrop, masks: undefined };
      const plain = await show(colour, "uncalibrated frame");
      const moved = await show({ ...colour, calibration: { ...neutral, redHue: 60, blueHue: -100, blueSaturation: 60 } }, "calibrated frame", plain);
      const tinted = await show({ ...colour, calibration: { ...neutral, shadowTint: 100 } }, "shadow-tinted frame", moved);
      // Mean change per channel, and how magenta (red and blue over green) a frame is.
      const change = (a: RenderedFrame | null, b: RenderedFrame | null) => {
        if (!a || !b) return null;
        const [p, q] = [a.frame.pixels, b.frame.pixels];
        let sum = 0;
        for (let i = 0; i < p.length; i += 4) sum += Math.abs(p[i]! - q[i]!) + Math.abs(p[i + 1]! - q[i + 1]!) + Math.abs(p[i + 2]! - q[i + 2]!);
        return Math.round((sum / ((p.length / 4) * 3)) * 10) / 10;
      };
      const magenta = (f: RenderedFrame | null) => {
        if (!f) return null;
        const px = f.frame.pixels;
        let sum = 0;
        for (let i = 0; i < px.length; i += 4) sum += (px[i]! + px[i + 2]!) / 2 - px[i + 1]!;
        return Math.round((sum / (px.length / 4)) * 10) / 10;
      };
      const luma = (f: RenderedFrame | null) => (f ? meanLuma(f) : null);
      return {
        colourChange: change(plain, moved),
        lumaChange: luma(plain) !== null && luma(moved) !== null ? Math.round((luma(moved)! - luma(plain)!) * 10) / 10 : null,
        magentaLift: magenta(tinted) !== null && magenta(plain) !== null ? Math.round((magenta(tinted)! - magenta(plain)!) * 10) / 10 : null,
        renderMs: moved?.frame.renderMs ?? null,
      };
    })();
    driver.editor().setRecipe(beforeCrop);
    const calibrationOk =
      calibrationCheck.colourChange !== null &&
      calibrationCheck.colourChange > 1 &&
      calibrationCheck.lumaChange !== null &&
      Math.abs(calibrationCheck.lumaChange) < 3 &&
      calibrationCheck.magentaLift !== null &&
      calibrationCheck.magentaLift > 0.5;

    // Remove (ADR 0066) on the real raw file: a short stroke is filled in; it changes
    // what it covers and nothing well outside it (sharpening spreads a few pixels);
    // with the fill cached, an exposure change renders about as fast as without it.
    const removeCheck = await (async () => {
      const base = { ...beforeCrop, masks: undefined, geometry: undefined, spots: undefined, removals: undefined };
      const plain = await show(base, "frame before removing");
      const stroke = { size: 0.008, feather: 10, flow: 100, points: [[0.3, 0.6], [0.38, 0.6]] as [number, number][] };
      const removal = { strokes: [stroke] };
      const t0 = performance.now();
      const removed = await show({ ...base, removals: [removal] }, "frame with a removal", plain);
      const firstMs = Math.round(performance.now() - t0);
      const cached = await show({ ...base, removals: [removal], exposure: base.exposure + 0.3 }, "exposure with a removal", plain);
      const brighter = await show({ ...base, exposure: base.exposure + 0.3 }, "exposure without a removal", plain);
      if (!plain || !removed) return { insideChange: null, outsideChange: null, firstMs, cachedMs: null, plainMs: null };
      const { width: w, height: h } = plain.frame;
      const r = stroke.size * Math.hypot(w, h);
      const [ax, bx, y0] = [stroke.points[0]![0] * w, stroke.points[1]![0] * w, stroke.points[0]![1] * h];
      const [p, q] = [plain.frame.pixels, removed.frame.pixels];
      let [inside, insideCount, outside] = [0, 0, 0];
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const px = Math.min(Math.max(x + 0.5, ax), bx);
          const d = Math.hypot(x + 0.5 - px, y + 0.5 - y0);
          const i = (y * w + x) * 4;
          const change = Math.abs(p[i]! - q[i]!) + Math.abs(p[i + 1]! - q[i + 1]!) + Math.abs(p[i + 2]! - q[i + 2]!);
          if (d < r * 0.8) {
            inside += change / 3;
            insideCount++;
          } else if (d > r + 8) outside = Math.max(outside, change);
        }
      }
      return {
        insideChange: Math.round((inside / Math.max(insideCount, 1)) * 10) / 10,
        outsideChange: outside,
        firstMs,
        cachedMs: cached?.frame.renderMs ?? null,
        plainMs: brighter?.frame.renderMs ?? null,
      };
    })();
    // One fill for every view (ADR 0070): until the removal is filled at full
    // resolution, frames say they show a stand-in; after, the whole photo and a window
    // at 100 % both show that fill.
    const fullFill = await (async () => {
      const image = driver.editor().image;
      if (!image) return null;
      const removal = { strokes: [{ size: 0.02, feather: 10, flow: 100, points: [[0.62, 0.35], [0.66, 0.38]] as [number, number][] }] };
      const recipe = { ...beforeCrop, masks: undefined, geometry: undefined, spots: undefined, removals: [removal] };
      const whole = () => ipc.renderPreview({ imageId: image.id, recipe, quality: "detail", targetLongEdge: 1600, slot: "compare" });
      const before = await whole();
      const t = performance.now();
      await ipc.prepareFill(image.id, [removal]);
      const fillMs = Math.round(performance.now() - t);
      const after = await whole();
      const [x, y] = [Math.round(0.6 * image.fullWidth), Math.round(0.3 * image.fullHeight)];
      const window = await ipc.renderPreview({ imageId: image.id, recipe, quality: "detail", targetLongEdge: Math.max(image.fullWidth, image.fullHeight), window: [x, y, 800, 600], slot: "compare" });
      return {
        standInFirst: before.fillPending,
        fillMs,
        wholePending: after.fillPending,
        windowPending: window.fillPending,
        wholeRenderMs: Math.round(after.renderMs),
        windowRenderMs: Math.round(window.renderMs),
        ok: before.fillPending && !after.fillPending && !window.fillPending && fillMs < 15_000,
      };
    })();
    driver.editor().setRecipe(beforeCrop);
    const removeOk =
      removeCheck.insideChange !== null &&
      removeCheck.insideChange > 0.2 &&
      removeCheck.outsideChange !== null &&
      removeCheck.outsideChange <= 6 &&
      removeCheck.cachedMs !== null &&
      removeCheck.plainMs !== null &&
      removeCheck.cachedMs < removeCheck.plainMs * 2 + 20;

    // Retouch (ADR 0054) on the real raw file: the engine finds a source for a heal
    // spot; the spot changes its disc and nothing outside it; with the spot cached, an
    // exposure change renders about as fast as without spots.
    const retouchCheck = await (async () => {
      const base = { ...beforeCrop, masks: undefined, geometry: undefined, spots: undefined };
      const plain = await show(base, "unretouched frame");
      const made = await ipc.newSpot(driver.editor().image!.id, "heal", [0.5, 0.45], 0.02, []).catch(() => null);
      if (!plain || !made) return { made: made !== null, insideChange: null, outsideChange: null, renderMs: null, cachedMs: null, plainMs: null };
      const healed = await show({ ...base, spots: [made] }, "healed frame", plain);
      const cached = await show({ ...base, spots: [made], exposure: base.exposure + 0.3 }, "exposure with a spot", plain);
      const brighter = await show({ ...base, exposure: base.exposure + 0.3 }, "exposure without spots", plain);
      if (!healed) return { made: true, insideChange: null, outsideChange: null, renderMs: null, cachedMs: null, plainMs: null };
      const { width: w, height: h } = plain.frame;
      const [cx, cy, r] = [made.x * w, made.y * h, made.radius * Math.max(w, h)];
      const [p, q] = [plain.frame.pixels, healed.frame.pixels];
      let [inside, insideCount, outside] = [0, 0, 0];
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const d = Math.hypot(x + 0.5 - cx, y + 0.5 - cy);
          const i = (y * w + x) * 4;
          const change = Math.abs(p[i]! - q[i]!) + Math.abs(p[i + 1]! - q[i + 1]!) + Math.abs(p[i + 2]! - q[i + 2]!);
          if (d < r * 0.9) {
            inside += change / 3;
            insideCount++;
          } else if (d > r * 1.1) outside = Math.max(outside, change);
        }
      }
      return {
        made: true,
        sourceDistance: Math.round(Math.hypot((made.sourceX - made.x) * w, (made.sourceY - made.y) * h) / r * 10) / 10,
        insideChange: Math.round((inside / Math.max(insideCount, 1)) * 10) / 10,
        outsideChange: outside,
        renderMs: healed.frame.renderMs,
        cachedMs: cached?.frame.renderMs ?? null,
        plainMs: brighter?.frame.renderMs ?? null,
      };
    })();
    driver.editor().setRecipe(beforeCrop);
    const retouchOk =
      retouchCheck.made &&
      retouchCheck.insideChange !== null &&
      retouchCheck.insideChange > 0.2 &&
      retouchCheck.outsideChange === 0;

    // A generated mask (ADR 0074) on the real raw file, where this computer makes them:
    // the engine finds the subject; +1 EV through it brightens where its view (what the
    // tint is drawn from) covers and changes nothing where it doesn't; and a mask made
    // from another photo (as a pasted edit's is) is reported missing, to be made again.
    const generatedCheck = await (async (): Promise<{
      supported: boolean;
      made?: boolean;
      ms?: number;
      share?: number;
      rendered?: boolean;
      coveredCells?: number;
      coveredGain?: number | null;
      outsideChange?: number;
      missingWhenMade?: string[] | null;
      foreignMissing?: number | null;
      renderMs?: number;
      skyIndoors?: boolean | null;
      skyMs?: number;
    }> => {
      const id = driver.editor().image!.id;
      if (!(driver.editor().info?.maskKinds ?? []).includes("subject")) return { supported: false };
      const t = performance.now();
      const made = await ipc.generateMask(id, "subject").catch(() => null);
      const ms = Math.round(performance.now() - t);
      if (!made) return { supported: true, made: false, ms };
      const base = { ...beforeCrop, masks: undefined, geometry: undefined, spots: undefined, removals: undefined };
      const lit = (name: string): EditRecipe => ({
        ...base,
        masks: [{ id: 1, shape: { kind: "generated", of: "subject", mask: name }, adjustments: { exposure: 1, warmth: 0, clarity: 0 } }],
      });
      const plain = await show(base, "frame without the subject mask");
      const masked = await show(lit(made.name), "subject mask frame", plain);
      const n = 48;
      const view = await ipc.maskView(id, null, made.name, { x: 0, y: 0, w: 1, h: 1 }, n, n).catch(() => new Uint8Array());
      const missing = await ipc.missingMasks(id, lit(made.name)).catch(() => null);
      const foreign = await ipc.missingMasks(id, lit("0".repeat(48))).catch(() => null);
      // The Z 6 is a still life indoors: no sky, however white its curtain.
      const tSky = performance.now();
      const sky = await ipc.generateMask(id, "sky").then(
        (m) => m !== null,
        () => null,
      );
      const skyMs = Math.round(performance.now() - tSky);
      driver.editor().setRecipe(beforeCrop);
      if (!plain || !masked || view.length !== n * n) {
        return { supported: true, made: true, ms, share: made.share, rendered: false };
      }
      // Each view cell's middle pixel, before and after.
      const { width: w, height: h } = plain.frame;
      let [gain, covered, outside] = [0, 0, 0];
      for (let j = 0; j < n; j++) {
        for (let i = 0; i < n; i++) {
          const k = (Math.floor(((j + 0.5) / n) * h) * w + Math.floor(((i + 0.5) / n) * w)) * 4;
          const [p, q] = [plain.frame.pixels, masked.frame.pixels];
          const change = (q[k]! - p[k]! + q[k + 1]! - p[k + 1]! + q[k + 2]! - p[k + 2]!) / 3;
          const v = view[j * n + i]!;
          if (v >= 230) {
            gain += change;
            covered++;
          } else if (v === 0) outside = Math.max(outside, Math.abs(change));
        }
      }
      return {
        supported: true,
        made: true,
        ms,
        share: Math.round(made.share * 1000) / 1000,
        rendered: true,
        coveredCells: covered,
        coveredGain: covered > 0 ? Math.round((gain / covered) * 10) / 10 : null,
        outsideChange: Math.round(outside * 10) / 10,
        missingWhenMade: missing,
        foreignMissing: foreign?.length ?? null,
        renderMs: masked.frame.renderMs,
        skyIndoors: sky,
        skyMs,
      };
    })();
    const generatedOk =
      !generatedCheck.supported ||
      (generatedCheck.made === true &&
        (generatedCheck.coveredGain ?? 0) > 10 &&
        (generatedCheck.outsideChange ?? Infinity) <= 2 &&
        generatedCheck.missingWhenMade?.length === 0 &&
        generatedCheck.foreignMissing === 1 &&
        generatedCheck.skyIndoors === false);

    // Sensor dust (ADR 0058) on the real raw file: found quickly, each a heal spot with a
    // source; once those are spots, nothing is left to find.
    const dustCheck = await (async () => {
      const id = driver.editor().image!.id;
      const t = performance.now();
      const found = await ipc.findDust(id, []).catch(() => null);
      const ms = Math.round((performance.now() - t) * 10) / 10;
      if (!found) return { found: null, ms, healed: false, leftAfterFixing: null };
      const healed = found.every((s) => s.kind === "heal" && (s.sourceX !== s.x || s.sourceY !== s.y));
      const left = found.length > 0 ? (await ipc.findDust(id, found)).length : 0;
      return { found: found.length, ms, healed, leftAfterFixing: left };
    })();
    const dustOk = dustCheck.found !== null && dustCheck.healed && dustCheck.leftAfterFixing === 0 && dustCheck.ms < 1000;

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
      zoom: zoom?.ok === true,
      focusMode: focusMode.ok,
      autoTone: autoTone?.ok === true,
      exportSizeEstimate: sizeEstimate !== null && Math.abs(sizeEstimate.errorPct) <= 30 && sizeEstimate.ms < 2000,
      framesDuringDrag: idleDrag.framesShown > 0,
      framesDuringExport: dragDuringExport.framesShown > 0,
      cacheHitOnReturn,
      noSchedulerErrors: stats.errors === 0,
      quitHeldDuringExport: quitGuard.held,
      libraryIndexed: indexOk,
      viewerSizeStableOnOpen: reopen.sizeStable,
      libraryThumbnails: thumbnailsOk,
      ratingsAndFlags: marksOk,
      colourLabels: labelsOk,
      allPhotosAndView: allPhotosOk,
      lastPlace: lastPlaceOk,
      albums: albumsOk,
      search: searchOk,
      savedEdits: editsOk,
      toneCurve: toneCurveOk,
      history: historyOk,
      compare: compareOk,
      presets: presetOk,
      batch: batchOk,
      exportQueue: exportQueueOk,
      exportFormats: formatsOk,
      outputSharpening: sharpeningOk,
      exportColourSpace: colourSpacesOk,
      exportMetadata: metadataOk,
      marksInExports: marksInExportOk,
      exportWatermark: watermarkOk,
      whiteBalanceLight: lightOk,
      colourGrading: gradingOk,
      calibration: calibrationOk,
      retouch: retouchOk,
      generatedMask: generatedOk,
      remove: removeOk,
      removalFill: fullFill?.ok === true,
      dust: dustOk,
      copyPaste: copyPasteOk,
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
      focusMode,
      autoTone,
      zoom,
      toneCurve,
      history,
      compare: compareCheck,
      presets: presetCheck,
      batch,
      exportQueue,
      whiteBalanceLight: lightCheck,
      colourGrading: gradingCheck,
      calibration: calibrationCheck,
      retouch: retouchCheck,
      generatedMask: generatedCheck,
      remove: removeCheck,
      removalFill: fullFill,
      dust: dustCheck,
      copyPaste: copyPasteCheck,
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
      allPhotos,
      albums,
      search: searchCheck,
      edits,
      detailFrame: { size: `${detail.frame.width}x${detail.frame.height}`, rustRenderMs: detail.frame.renderMs, roundTripMs: detail.info.roundTripMs },
      baselineUiFrameGapMs: summarise(baselineGaps),
      idleDrag,
      cacheHitOnReturn,
      dragDuringExport,
      export: { wallMs: exportWallMs, result: finished, sizeEstimate },
      scheduler: stats,
      devicePixelRatio: window.devicePixelRatio,
    };
  } catch (e) {
    const message = e instanceof Error ? e.message : typeof e === "object" && e !== null && "message" in e ? String(e.message) : String(e);
    return { ok: false, error: message, scheduler: driver.editor().schedulerStats() };
  } finally {
    unsubscribe();
  }
}
