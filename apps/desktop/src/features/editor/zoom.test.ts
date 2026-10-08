import { describe, expect, it } from "vitest";
import { clampCentre, drawnPart, fitScale, needsFull, panned, pointAt, sameWindow, steppedScale, visiblePart, zoomKeeping, zoomLayout, zoomPercent } from "./zoom";

const view = { width: 1000, height: 600 };
const full = { width: 6000, height: 4000 };
const at100 = (x: number, y: number) => ({ centre: { x, y }, scale: 1 });

describe("zoomLayout", () => {
  it("shows the viewport's worth of device pixels around the centre", () => {
    const z = zoomLayout(view, full, 2, at100(0.5, 0.5));
    expect([z.width, z.height]).toEqual([3000, 2000]);
    expect([z.left, z.top]).toEqual([500 - 1500, 300 - 1000]);
    expect(z.window).toEqual([2000, 1400, 2000, 1200]);
  });

  it("covers more of the photo at a lower zoom, and less at a higher one", () => {
    const half = zoomLayout(view, full, 1, { centre: { x: 0.5, y: 0.5 }, scale: 0.5 });
    expect([half.width, half.height]).toEqual([3000, 2000]);
    expect(half.window).toEqual([2000, 1400, 2000, 1200]);
    const double = zoomLayout(view, full, 1, { centre: { x: 0.5, y: 0.5 }, scale: 2 });
    expect(double.window).toEqual([2750, 1850, 500, 300]);
  });

  it("never uncovers the photo's edges", () => {
    const corner = zoomLayout(view, full, 1, at100(0, 0));
    expect([corner.left, corner.top]).toEqual([0, 0]);
    expect(corner.window).toEqual([0, 0, 1000, 600]);
    const far = zoomLayout(view, full, 1, at100(1, 1));
    expect(far.window).toEqual([5000, 3400, 1000, 600]);
  });

  it("centres a photo smaller than the viewport and shows all of it", () => {
    const z = zoomLayout(view, { width: 800, height: 400 }, 1, at100(0.9, 0.1));
    expect([z.left, z.top]).toEqual([100, 100]);
    expect(z.window).toEqual([0, 0, 800, 400]);
  });

  it("lands on whole device pixels", () => {
    const z = zoomLayout(view, full, 2, at100(0.50013, 0.5));
    expect(Number.isInteger(z.left * 2)).toBe(true);
    expect(Number.isInteger(z.window[0])).toBe(true);
  });
});

describe("panning and zooming", () => {
  it("moves the photo with the drag, within its edges", () => {
    const c = panned(view, full, 1, at100(0.5, 0.5), 600, 0);
    expect(c.centre.x).toBeCloseTo(0.4);
    expect(panned(view, full, 1, at100(0.5, 0.5), 1e6, -1e6).centre).toEqual(clampCentre(view, full, 1, { x: 0, y: 1 }, 1));
  });

  it("keeps the point under the pointer while the zoom changes", () => {
    const pointer = { x: 200, y: 400 };
    const before = { centre: { x: 0.4, y: 0.55 }, scale: 0.5 };
    const at = pointAt(view, full, 1, before, pointer);
    for (const scale of [0.7, 1, 3]) {
      const z = zoomLayout(view, full, 1, zoomKeeping(view, full, 1, at, pointer, scale));
      expect(z.left + at.x * z.width).toBeCloseTo(pointer.x, 0);
      expect(z.top + at.y * z.height).toBeCloseTo(pointer.y, 0);
    }
  });

  it("knows the scale that fits, and steps between levels above it", () => {
    expect(fitScale(view, full, 2)).toBeCloseTo(0.3);
    const fit = fitScale(view, full, 1);
    expect(steppedScale(fit, 1, fit)).toBe(1 / 4);
    expect(steppedScale(1 / 4, 1, fit)).toBe(1 / 3);
    expect(steppedScale(0.8, 1, fit)).toBe(1);
    expect(steppedScale(1, -1, fit)).toBe(2 / 3);
    expect(steppedScale(1 / 4, -1, fit)).toBeNull();
    expect(steppedScale(8, 1, fit)).toBeNull();
    expect(zoomPercent(2 / 3)).toBe("67%");
  });
});

it("decodes the full resolution only for zooms past the largest preview", () => {
  // A 6000 px photo with a 3000 px preview, cropped to 4000 px.
  expect(needsFull([3000, 2000], [6000, 4000], 2000, 4000)).toBe(false);
  expect(needsFull([3000, 2000], [6000, 4000], 2100, 4000)).toBe(true);
  expect(needsFull([3000, 2000], [6000, 4000], 4000, 4000)).toBe(true);
});

it("compares windows by value", () => {
  expect(sameWindow([1, 2, 3, 4], [1, 2, 3, 4])).toBe(true);
  expect(sameWindow([1, 2, 3, 4], [1, 2, 3, 5])).toBe(false);
  expect(sameWindow(null, null)).toBe(true);
  expect(sameWindow(null, [1, 2, 3, 4])).toBe(false);
});

describe("overlays when zoomed", () => {
  it("know the part of the photo in view", () => {
    const z = zoomLayout(view, full, 1, at100(0.5, 0.5));
    const [x0, y0, x1, y1] = visiblePart(view, z);
    expect(x0).toBeCloseTo(2500 / 6000);
    expect(x1).toBeCloseTo(3500 / 6000);
    expect(y0).toBeCloseTo(1700 / 4000);
    expect(y1).toBeCloseTo(2300 / 4000);
  });

  it("draw a little more than is in view, the same for small pans", () => {
    const zoom = at100(0.513, 0.487);
    const visible = visiblePart(view, zoomLayout(view, full, 1, zoom));
    const drawn = drawnPart(visible);
    expect(drawn[0]).toBeLessThan(visible[0]);
    expect(drawn[2]).toBeGreaterThan(visible[2]);
    const nudged = visiblePart(view, zoomLayout(view, full, 1, panned(view, full, 1, zoom, 3, 2)));
    expect(drawnPart(nudged)).toEqual(drawn);
    expect(drawnPart([0, 0, 1, 1])).toEqual([0, 0, 1, 1]);
  });
});
