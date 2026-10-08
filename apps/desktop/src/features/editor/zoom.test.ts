import { describe, expect, it } from "vitest";
import { centreKeeping, clampCentre, drawnPart, panned, sameWindow, visiblePart, zoomLayout } from "./zoom";

const view = { width: 1000, height: 600 };
const full = { width: 6000, height: 4000 };

describe("zoomLayout", () => {
  it("shows the viewport's worth of device pixels around the centre", () => {
    const z = zoomLayout(view, full, 2, { x: 0.5, y: 0.5 });
    expect([z.width, z.height]).toEqual([3000, 2000]);
    expect([z.left, z.top]).toEqual([500 - 1500, 300 - 1000]);
    expect(z.window).toEqual([2000, 1400, 2000, 1200]);
  });

  it("never uncovers the photo's edges", () => {
    const corner = zoomLayout(view, full, 1, { x: 0, y: 0 });
    expect([corner.left, corner.top]).toEqual([0, 0]);
    expect(corner.window).toEqual([0, 0, 1000, 600]);
    const far = zoomLayout(view, full, 1, { x: 1, y: 1 });
    expect(far.window).toEqual([5000, 3400, 1000, 600]);
  });

  it("centres a photo smaller than the viewport and shows all of it", () => {
    const z = zoomLayout(view, { width: 800, height: 400 }, 1, { x: 0.9, y: 0.1 });
    expect([z.left, z.top]).toEqual([100, 100]);
    expect(z.window).toEqual([0, 0, 800, 400]);
  });

  it("lands on whole device pixels", () => {
    const z = zoomLayout(view, full, 2, { x: 0.50013, y: 0.5 });
    expect(Number.isInteger(z.left * 2)).toBe(true);
    expect(Number.isInteger(z.window[0])).toBe(true);
  });
});

describe("panning", () => {
  it("moves the photo with the drag, within its edges", () => {
    const c = panned(view, full, 1, { x: 0.5, y: 0.5 }, 600, 0);
    expect(c.x).toBeCloseTo(0.4);
    expect(panned(view, full, 1, { x: 0.5, y: 0.5 }, 1e6, -1e6)).toEqual(clampCentre(view, full, 1, { x: 0, y: 1 }));
  });

  it("keeps the clicked point under the pointer", () => {
    const at = { x: 0.3, y: 0.6 };
    const pointer = { x: 200, y: 400 };
    const c = centreKeeping(view, full, 1, at, pointer);
    const z = zoomLayout(view, full, 1, c);
    expect(z.left + at.x * z.width).toBeCloseTo(pointer.x, 0);
    expect(z.top + at.y * z.height).toBeCloseTo(pointer.y, 0);
  });
});

it("compares windows by value", () => {
  expect(sameWindow([1, 2, 3, 4], [1, 2, 3, 4])).toBe(true);
  expect(sameWindow([1, 2, 3, 4], [1, 2, 3, 5])).toBe(false);
  expect(sameWindow(null, null)).toBe(true);
  expect(sameWindow(null, [1, 2, 3, 4])).toBe(false);
});

describe("overlays at 100 %", () => {
  it("know the part of the photo in view", () => {
    const z = zoomLayout(view, full, 1, { x: 0.5, y: 0.5 });
    const [x0, y0, x1, y1] = visiblePart(view, z);
    expect(x0).toBeCloseTo(2500 / 6000);
    expect(x1).toBeCloseTo(3500 / 6000);
    expect(y0).toBeCloseTo(1700 / 4000);
    expect(y1).toBeCloseTo(2300 / 4000);
  });

  it("draw a little more than is in view, the same for small pans", () => {
    const centre = { x: 0.513, y: 0.487 };
    const visible = visiblePart(view, zoomLayout(view, full, 1, centre));
    const drawn = drawnPart(visible);
    expect(drawn[0]).toBeLessThan(visible[0]);
    expect(drawn[2]).toBeGreaterThan(visible[2]);
    const nudged = visiblePart(view, zoomLayout(view, full, 1, panned(view, full, 1, centre, 3, 2)));
    expect(drawnPart(nudged)).toEqual(drawn);
    expect(drawnPart([0, 0, 1, 1])).toEqual([0, 0, 1, 1]);
  });
});
