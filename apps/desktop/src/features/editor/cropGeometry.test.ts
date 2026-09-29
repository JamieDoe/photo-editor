import { describe, expect, it } from "vitest";
import { cropView, drag, fitCrop, fromView, largestIn, remap, toView, viewRatio } from "./cropGeometry";

describe("crop geometry", () => {
  it("fits crops like the renderer", () => {
    // The same case as `fit_crop_matches_the_ui` in crates/renderer/src/geometry.rs.
    const c = fitCrop(1.5, 5, 6000, 4000);
    expect(c.w).toBeCloseTo(0.88737, 4);
    expect(c.h).toBeCloseTo(0.88737, 4);
    expect(c.x).toBeCloseTo(0.05632, 4);
    expect(cropView(0, 6000, 4000)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });

  it("maps between the photo and the view", () => {
    const v = cropView(8, 6000, 4000);
    const c = { x: 0.3, y: 0.25, w: 0.4, h: 0.5 };
    const back = fromView(toView(c, v), v);
    for (const k of ["x", "y", "w", "h"] as const) expect(back[k]).toBeCloseTo(c[k], 9);
    // Remapping between views keeps the place in the view.
    const v2 = cropView(-3, 6000, 4000);
    const moved = toView(remap(c, v, v2), v2);
    const before = toView(c, v);
    for (const k of ["x", "y", "w", "h"] as const) expect(moved[k]).toBeCloseTo(before[k], 9);
  });

  it("fits a shape inside the view", () => {
    const v = cropView(0, 6000, 4000);
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
