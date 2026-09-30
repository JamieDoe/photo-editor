import { describe, expect, it } from "vitest";
import type { Geometry } from "../../ipc/generated/Geometry";
import {
  cropView,
  drag,
  fitCrop,
  fitCropFor,
  flipGeometry,
  fromView,
  largestIn,
  orientedSize,
  remap,
  toView,
  turnGeometry,
  viewRatio,
  viewToSource,
} from "./cropGeometry";

const shape = (straighten: number, vertical = 0, horizontal = 0) => ({ straighten, vertical, horizontal });

describe("crop geometry", () => {
  const g: Geometry = {
    straighten: 3,
    crop: { x: 0.1, y: 0.2, w: 0.5, h: 0.3 },
    aspect: "wide16x9",
    vertical: 40,
    horizontal: -10,
    rotation: 0,
    flip: false,
  };

  it("turns the edit with the picture", () => {
    const cw = turnGeometry(g, 1);
    expect(cw.rotation).toBe(1);
    // The crop's top-left corner goes to the top-right.
    expect(cw.crop.x).toBeCloseTo(0.5);
    expect(cw.crop.y).toBeCloseTo(0.1);
    expect([cw.crop.w, cw.crop.h]).toEqual([0.3, 0.5]);
    expect([cw.vertical, cw.horizontal, cw.straighten, cw.aspect]).toEqual([10, 40, 3, "free"]);
    // Four turns either way come back.
    let back = g;
    for (let i = 0; i < 4; i++) back = turnGeometry(back, 1);
    expect({ ...back, aspect: g.aspect }).toEqual(g);
    const undone = turnGeometry(turnGeometry(g, 1), -1);
    expect(undone.crop.x).toBeCloseTo(g.crop.x);
    expect(undone.crop.y).toBeCloseTo(g.crop.y);
    expect([undone.vertical, undone.horizontal]).toEqual([g.vertical, g.horizontal]);
    expect(orientedSize(cw, 6000, 4000)).toEqual({ width: 4000, height: 6000 });
  });

  it("mirrors the edit with the picture", () => {
    const f = flipGeometry(turnGeometry(g, 1));
    expect([f.flip, f.rotation]).toEqual([true, 3]);
    expect(f.crop.x).toBeCloseTo(0.2);
    expect([f.straighten, f.horizontal]).toEqual([-3, -40]);
    expect(flipGeometry(flipGeometry(g))).toEqual(g);
  });

  it("fits crops like the renderer", () => {
    // The same case as `fit_crop_matches_the_ui` in crates/renderer/src/geometry.rs.
    const c = fitCrop(1.5, 5, 6000, 4000);
    expect(c.w).toBeCloseTo(0.88737, 4);
    expect(c.h).toBeCloseTo(0.88737, 4);
    expect(c.x).toBeCloseTo(0.05632, 4);
    expect(cropView(shape(0), 6000, 4000)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });

  it("fits crops with perspective like the renderer", () => {
    // The same case as `fit_crop_for_matches_the_ui` in crates/renderer/src/geometry.rs.
    const c = fitCropFor(1.5, shape(3, 40, -20), 6000, 4000);
    expect(c.w).toBeCloseTo(0.84362, 4);
    expect(c.h).toBeCloseTo(0.84362, 4);
  });

  it("widens the top for positive Vertical, about the centre", () => {
    const map = viewToSource(shape(0, 50), 6000, 4000);
    const top = map(1, 0)[0] - map(0, 0)[0];
    const bottom = map(1, 1)[0] - map(0, 1)[0];
    expect(top).toBeLessThan(0.9 * bottom);
    const [x, y] = map(0.5, 0.5);
    expect(x).toBeCloseTo(3000, 2);
    expect(y).toBeCloseTo(2000, 2);
  });

  it("maps between the photo and the view", () => {
    const v = cropView(shape(8), 6000, 4000);
    const c = { x: 0.3, y: 0.25, w: 0.4, h: 0.5 };
    const back = fromView(toView(c, v), v);
    for (const k of ["x", "y", "w", "h"] as const) expect(back[k]).toBeCloseTo(c[k], 9);
    // Remapping between views keeps the place in the view.
    const v2 = cropView(shape(-3, 25, 10), 6000, 4000);
    const moved = toView(remap(c, v, v2), v2);
    const before = toView(c, v);
    for (const k of ["x", "y", "w", "h"] as const) expect(moved[k]).toBeCloseTo(before[k], 9);
  });

  it("fits a shape inside the view", () => {
    const v = cropView(shape(0), 6000, 4000);
    const square = largestIn(viewRatio(1, v, 6000, 4000));
    expect(square.h).toBeCloseTo(1);
    expect(square.w * 6000).toBeCloseTo(square.h * 4000);
  });

  it("drags freely within the view", () => {
    const start = { x: 0.2, y: 0.2, w: 0.5, h: 0.5 };
    expect(drag(start, "move", 0.5, -0.5, null)).toEqual({ x: 0.5, y: 0, w: 0.5, h: 0.5 });
    const se = drag(start, "se", 0.1, 0.05, null);
    expect(se.w).toBeCloseTo(0.6);
    expect(se.h).toBeCloseTo(0.55);
    expect(se.x).toBe(0.2);
  });

  it("keeps a locked shape and the opposite corner", () => {
    const start = { x: 0.1, y: 0.1, w: 0.4, h: 0.4 };
    const r = drag(start, "se", 0.3, 0.05, 1);
    expect(r.w).toBeCloseTo(r.h);
    expect([r.x, r.y]).toEqual([0.1, 0.1]);
    // Growing past the view's edge stops at it, still in shape.
    const big = drag(start, "se", 2, 2, 1);
    expect(big.x + big.w).toBeLessThanOrEqual(1 + 1e-9);
    expect(big.w).toBeCloseTo(big.h);
  });
});
