import { describe, expect, it } from "vitest";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { FrameHistogram } from "../../ipc/frame";
import { draggedValue, histogramPaths, zoneAt } from "./histogramGraph";

function histogram(fill: (i: number) => number): FrameHistogram {
  const plane = () => Uint32Array.from({ length: 256 }, (_, i) => fill(i));
  return { red: plane(), green: plane(), blue: plane(), luma: plane() };
}

const exposure: AdjustmentSpec = { key: "exposure", label: "Exposure", group: "Light", min: -5, max: 5, step: 0.01, default: 0, more: false, unit: "EV" };
const shadows: AdjustmentSpec = { key: "shadows", label: "Shadows", group: "Light", min: -100, max: 100, step: 1, default: 0, more: false, unit: "" };

describe("histogram graph", () => {
  it("draws filled channels and a luminance line across the width", () => {
    const p = histogramPaths(histogram((i) => 100 + i));
    expect(p.red.startsWith("M0 72")).toBe(true);
    expect(p.red.endsWith("L288 72 Z")).toBe(true);
    expect(p.luma.startsWith("M0.0")).toBe(true);
    // 128 points on the line.
    expect(p.luma.split("L").length).toBe(128);
    // The tallest inner point reaches the top margin.
    expect(p.luma).toContain(" 6.0");
  });

  it("cuts off spikes instead of flattening the rest", () => {
    // A flat area: one value holds half the pixels.
    const p = histogramPaths(histogram((i) => (i === 100 ? 25_000 : 100)));
    const heights = p.luma
      .split("L")
      .map((s) => Number(s.trim().split(" ")[1]));
    // The broad part still stands clear of the floor (72), not squashed to it.
    expect(Math.min(...heights.filter((y) => y > 6))).toBeLessThan(60);
  });

  it("lights the clipping triangles only for a real share of clipped pixels", () => {
    const clean = histogramPaths(histogram((i) => (i === 0 || i === 255 ? 0 : 100)));
    expect([clean.shadowsClipped, clean.highlightsClipped]).toEqual([false, false]);
    const blown = histogramPaths(histogram((i) => (i === 0 ? 0 : i === 255 ? 500 : 100)));
    expect([blown.shadowsClipped, blown.highlightsClipped]).toEqual([false, true]);
    // One pixel in a million does not count.
    const speck = histogram((i) => (i === 0 ? 0 : 20_000));
    speck.red[0] = 1;
    expect(histogramPaths(speck).shadowsClipped).toBe(false);
  });

  it("splits the width into Lightroom's zones", () => {
    expect([0, 0.05, 0.2, 0.5, 0.8, 0.95, 1].map(zoneAt)).toEqual([
      "blacks",
      "blacks",
      "shadows",
      "exposure",
      "highlights",
      "whites",
      "whites",
    ]);
  });

  it("drags by half the range across the width, in steps, within range", () => {
    expect(draggedValue(exposure, 0, 0.1)).toBe(0.5);
    expect(draggedValue(exposure, 0.3, -0.02)).toBe(0.2);
    expect(draggedValue(exposure, 4.8, 0.5)).toBe(5);
    expect(draggedValue(shadows, 10, 0.123)).toBe(22);
    expect(draggedValue(shadows, -90, -1)).toBe(-100);
  });
});
