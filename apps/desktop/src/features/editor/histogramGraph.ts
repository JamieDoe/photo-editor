import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { FrameHistogram } from "../../ipc/frame";

/**
 * The panel's histogram (ADR 0036): drawing the rendered frame's counts, and the
 * Lightroom-style zones that drag the tone sliders.
 */

export const GRAPH_WIDTH = 288;
export const GRAPH_HEIGHT = 72;
/** Points per drawn curve: pairs of the 256 bins. */
const POINTS = 128;
/** Heights above this multiple of the average are cut off. */
const SPIKE_CAP = 5;
/** A clipping triangle lights when this share of pixels is at 0 (or 255). */
const CLIP_SHARE = 0.0002;

export interface HistogramPaths {
  red: string;
  green: string;
  blue: string;
  luma: string;
  shadowsClipped: boolean;
  highlightsClipped: boolean;
}

/** SVG paths for the graph: red, green and blue filled, luminance as a line; each
 *  lightly smoothed and scaled so the tallest inner bin fills the height. Spikes (at
 *  pure black or white, or from a large flat area such as sky) are cut off at five
 *  times the typical height, as they would flatten everything else. */
export function histogramPaths(h: FrameHistogram): HistogramPaths {
  const [red, green, blue, luma] = [smoothPairs(h.red), smoothPairs(h.green), smoothPairs(h.blue), smoothPairs(h.luma)];
  const inner = [red, green, blue, luma].map((p) => p.slice(1, POINTS - 1));
  // The average without each curve's three tallest points, where a spike would be.
  const typical = inner.map((p) => [...p].sort((a, b) => a - b).slice(0, -3));
  const average = typical.reduce((sum, p) => sum + p.reduce((a, b) => a + b, 0), 0) / (typical.length * (POINTS - 5));
  const tallest = Math.max(1, Math.min(SPIKE_CAP * average, ...inner.map((p) => Math.max(...p))));
  const top = 6;
  const toPath = (p: Float64Array, closed: boolean) => {
    let d = closed ? `M0 ${GRAPH_HEIGHT}` : "";
    p.forEach((v, i) => {
      const x = (i / (POINTS - 1)) * GRAPH_WIDTH;
      const y = GRAPH_HEIGHT - Math.min(1, v / tallest) * (GRAPH_HEIGHT - top);
      d += `${closed || i > 0 ? " L" : "M"}${x.toFixed(1)} ${y.toFixed(1)}`;
    });
    return closed ? `${d} L${GRAPH_WIDTH} ${GRAPH_HEIGHT} Z` : d;
  };
  const total = h.luma.reduce((a, b) => a + b, 0);
  const share = (n: number) => (total > 0 ? n / total : 0);
  const last = h.red.length - 1;
  return {
    red: toPath(red, true),
    green: toPath(green, true),
    blue: toPath(blue, true),
    luma: toPath(luma, false),
    shadowsClipped: share(Math.max(h.red[0] ?? 0, h.green[0] ?? 0, h.blue[0] ?? 0)) >= CLIP_SHARE,
    highlightsClipped: share(Math.max(h.red[last] ?? 0, h.green[last] ?? 0, h.blue[last] ?? 0)) >= CLIP_SHARE,
  };
}

/** Pairs of bins summed, then a [1 2 1] blur. */
function smoothPairs(bins: Uint32Array): Float64Array {
  const at = (i: number) => bins[i] ?? 0;
  const pairs = Array.from({ length: POINTS }, (_, i) => at(2 * i) + at(2 * i + 1));
  const p = (i: number) => pairs[Math.min(POINTS - 1, Math.max(0, i))] ?? 0;
  return Float64Array.from({ length: POINTS }, (_, i) => (p(i - 1) + 2 * p(i) + p(i + 1)) / 4);
}

/** The tone sliders a drag on the graph moves, left to right, as in Lightroom. */
export const ZONES: ReadonlyArray<{ key: string; from: number; to: number }> = [
  { key: "blacks", from: 0, to: 0.1 },
  { key: "shadows", from: 0.1, to: 0.35 },
  { key: "exposure", from: 0.35, to: 0.65 },
  { key: "highlights", from: 0.65, to: 0.9 },
  { key: "whites", from: 0.9, to: 1 },
];

/** The zone under `fraction` (0..1 across the graph). */
export function zoneAt(fraction: number): string {
  const f = Math.min(Math.max(fraction, 0), 0.999999);
  return ZONES.find((z) => f >= z.from && f < z.to)!.key;
}

/** A slider's value after dragging by `dx` (a fraction of the graph's width, right is
 *  positive) from `start`: the whole width moves half the slider's range, rounded to
 *  its step and kept in range. */
export function draggedValue(spec: AdjustmentSpec, start: number, dx: number): number {
  const raw = start + dx * ((spec.max - spec.min) / 2);
  const stepped = Math.round(raw / spec.step) * spec.step;
  // Round away float noise from the step (0.1 + 0.2).
  const clean = Number(stepped.toFixed(6));
  return Math.min(spec.max, Math.max(spec.min, clean));
}
