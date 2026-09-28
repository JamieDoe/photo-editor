import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { ExportEvent } from "../../ipc/generated/ExportEvent";
import type { SelfTestConfigDto } from "../../ipc/generated/SelfTestConfigDto";
import type { DisplayedFrame, Editor } from "../editor/useEditor";

type RenderedFrame = Extract<DisplayedFrame, { source: "render" }>;
type EmbeddedShown = Extract<DisplayedFrame, { source: "embedded" }>;

/**
 * End-to-end self-test, run only when the app is launched with PE_SELF_TEST=<file>.
 * Drives the real UI state (same actions as the sliders) through real IPC and
 * reports UI-side timings to stdout via Rust. See docs/PERFORMANCE.md.
 */
export interface SelfTestDriver {
  editor: () => Editor;
}

const nextFrame = () => new Promise<number>((r) => requestAnimationFrame(r));
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
  const embedded: EmbeddedShown[] = [];
  // Display order, to check the invariant: once a render of an opened image is shown,
  // no embedded preview may follow it within that open.
  const events: Array<{ kind: "render"; imageId: number } | { kind: "embedded" }> = [];
  const unsubscribe = driver.editor().subscribeFrames((f) => {
    if (f.source === "render") {
      frames.push(f);
      events.push({ kind: "render", imageId: f.imageId });
    } else {
      embedded.push(f);
      events.push({ kind: "embedded" });
    }
  });
  /** Whether an embedded preview was shown after a render of `imageId`, looking only
   * at events from `fromIndex` (the start of that open). */
  const embeddedAfterRenderOf = (imageId: number, fromIndex: number) => {
    const firstRender = events.findIndex((e, i) => i >= fromIndex && e.kind === "render" && e.imageId === imageId);
    return firstRender >= 0 && events.slice(firstRender).some((e) => e.kind === "embedded");
  };
  let embeddedAfterRender = false;
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

    const openEventsStart = events.length;
    const tOpen = performance.now();
    const summary = await driver.editor().openPath(config.imagePath);
    if (!summary) throw new Error(driver.editor().error ?? "open failed");
    const openIpcMs = performance.now() - tOpen;
    const first = await waitFor(() => frames[0], 10_000, "first frame");
    const firstFrameMs = performance.now() - tOpen;
    const firstEmbedded = embedded[0];
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
    const embeddedBefore = embedded.length;
    const framesBefore = frames.length;
    // Checked for the first open once all its frames (including late ones) are in.
    embeddedAfterRender ||= embeddedAfterRenderOf(summary.id, openEventsStart);
    const reopenEventsStart = events.length;
    const tReopen = performance.now();
    const reopened = await driver.editor().openPath(config.imagePath);
    if (!reopened) throw new Error(driver.editor().error ?? "re-open failed");
    await waitFor(() => (frames.length > framesBefore ? true : null), 10_000, "re-open frame");
    const reopenEmbedded = embedded[embeddedBefore];
    await sleep(300); // let any late channel message arrive before checking order
    embeddedAfterRender ||= embeddedAfterRenderOf(reopened.id, reopenEventsStart);
    const reopen = {
      embeddedShownMs: reopenEmbedded?.sinceOpenMs ?? null,
      embeddedExtractMs: reopened.embeddedPreviewMs,
      decodeMs: reopened.decodeMs,
      firstRenderMs: performance.now() - tReopen,
    };

    const stats = driver.editor().schedulerStats();
    const ok =
      finished.type === "finished" &&
      idleDrag.framesShown > 0 &&
      dragDuringExport.framesShown > 0 &&
      cacheHitOnReturn &&
      !embeddedAfterRender &&
      stats.errors === 0;
    return {
      ok,
      file: summary.fileName,
      codecs: { jpegEncoder: info.jpegEncoder, embeddedJpegDecoder: info.embeddedJpegDecoder },
      camera: summary.camera,
      fullSize: `${summary.fullWidth}x${summary.fullHeight}`,
      pyramid: summary.levels.map(([w, h]) => `${w}x${h}`),
      open: { decodeMs: summary.decodeMs, pyramidMs: summary.pyramidMs, identityMs: summary.identityMs, ipcTotalMs: openIpcMs },
      firstFrame: { ms: firstFrameMs, size: `${first.frame.width}x${first.frame.height}`, quality: first.info.quality },
      embeddedPreview: firstEmbedded
        ? { shownAfterMs: firstEmbedded.sinceOpenMs, extractMs: summary.embeddedPreviewMs, size: `${firstEmbedded.frame.width}x${firstEmbedded.frame.height}` }
        : null,
      firstVisibleMs: firstEmbedded ? Math.min(firstEmbedded.sinceOpenMs, firstFrameMs) : firstFrameMs,
      embeddedAfterRender,
      reopen,
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
