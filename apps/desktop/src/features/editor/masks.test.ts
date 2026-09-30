import { describe, expect, it } from "vitest";
import type { Mask } from "../../ipc/generated/Mask";
import {
  FULL_CROP,
  flipMasks,
  fromShown,
  linearCoverage,
  maskName,
  brushSizeFromSlider,
  newBrushMask,
  newLinearMask,
  sliderFromBrushSize,
  newRadialMask,
  radialCoverage,
  type Point,
  setAdjustment,
  toShown,
  turnMasks,
  withMasks,
} from "./masks";
import { neutralRecipe } from "./recipe";

/** A linear shape's start point (the tests make linear masks). */
const startOf = (m: Mask) => (m.shape.kind === "linear" ? m.shape.start : null);

describe("masks", () => {
  it("adds numbered linear gradients over the shown picture", () => {
    const crop = { x: 0.2, y: 0.1, w: 0.6, h: 0.8 };
    const a = newLinearMask([], crop);
    expect(a.id).toBe(1);
    // In frame fractions, inside the crop.
    expect(startOf(a)![0]).toBeCloseTo(0.5);
    expect(startOf(a)![1]).toBeCloseTo(0.18);
    const b = newLinearMask([a], crop);
    expect(b.id).toBe(2);
    expect(maskName([a], a)).toBe("Linear gradient");
    expect(maskName([a, b], b)).toBe("Linear gradient 2");
  });

  it("moves points between the frame and the shown crop", () => {
    const crop = { x: 0.25, y: 0, w: 0.5, h: 1 };
    const shown = toShown([0.5, 0.3], crop);
    expect(shown).toEqual([0.5, 0.3]);
    const back = fromShown(toShown([0.3, 0.7], crop), crop);
    expect(back[0]).toBeCloseTo(0.3, 9);
    expect(back[1]).toBeCloseTo(0.7, 9);
  });

  it("turns and mirrors with the picture", () => {
    const m: Mask = { id: 1, shape: { kind: "linear", start: [0.2, 0.1], end: [0.2, 0.6] }, adjustments: { exposure: 0, warmth: 0, clarity: 0 } };
    const cw = turnMasks([m], 1)[0]!;
    // Top-left goes to the top-right.
    expect(startOf(cw)).toEqual([0.9, 0.2]);
    const back = turnMasks(turnMasks([m], 1), -1)[0]!;
    expect(back.shape).toEqual(m.shape);
    expect(startOf(flipMasks([m])[0]!)).toEqual([0.8, 0.1]);
  });

  it("keeps the recipe's masks tidy", () => {
    const r = neutralRecipe(17);
    const m = newLinearMask([], FULL_CROP);
    const one = withMasks(r, setAdjustment([m], 1, "exposure", -0.5));
    expect(one.masks?.[0]?.adjustments.exposure).toBe(-0.5);
    expect(withMasks(one, []).masks).toBeUndefined();
  });

  it("adds radial gradients and turns them with the picture", () => {
    const m = newRadialMask([], FULL_CROP);
    expect(m.shape.kind).toBe("radial");
    const turned = turnMasks([{ ...m, shape: { ...(m.shape as Extract<Mask["shape"], { kind: "radial" }>), centre: [0.2, 0.1], angle: 120 } }], 1)[0]!;
    const shape = turned.shape as Extract<Mask["shape"], { kind: "radial" }>;
    expect(shape.centre).toEqual([0.9, 0.2]);
    // 120 + 90 = 210 = -150.
    expect(shape.angle).toBe(-150);
    const flipped = flipMasks([turned])[0]!.shape as Extract<Mask["shape"], { kind: "radial" }>;
    expect([flipped.centre[0], flipped.angle]).toEqual([0.1, 150]);
  });

  it("covers radially as the renderer does", () => {
    // The renderer's test: 300 x 400 frame (diagonal 500), radii 100 and 50 px.
    const shape = { kind: "radial" as const, centre: [0.5, 0.5] as Point, radius: [0.2, 0.1] as Point, angle: 0, feather: 50 };
    expect(radialCoverage(shape, [0.5, 0.5], 300, 400)).toBe(1);
    expect(radialCoverage(shape, [(150 + 101) / 300, 0.5], 300, 400)).toBe(0);
    expect(radialCoverage(shape, [(150 + 75) / 300, 0.5], 300, 400)).toBeCloseTo(0.5, 5);
    expect(radialCoverage({ ...shape, angle: 90 }, [0.5, (200 + 75) / 400], 300, 400)).toBeCloseTo(0.5, 5);
  });

  it("adds brush masks and turns their strokes with the picture", () => {
    const m = newBrushMask([]);
    expect(m.shape).toEqual({ kind: "brush", strokes: [] });
    const painted: Mask = {
      ...m,
      shape: { kind: "brush", strokes: [{ size: 0.04, feather: 50, flow: 100, points: [[0.2, 0.1], [0.3, 0.1]] }] },
    };
    const turned = turnMasks([painted], 1)[0]!;
    expect(turned.shape.kind === "brush" && turned.shape.strokes[0]!.points).toEqual([
      [0.9, 0.2],
      [0.9, 0.3],
    ]);
    // Sizes are diagonal fractions: a turn keeps them.
    expect(turned.shape.kind === "brush" && turned.shape.strokes[0]!.size).toBe(0.04);
    expect(brushSizeFromSlider(sliderFromBrushSize(0.04))).toBe(0.04);
  });

  it("covers as the renderer does", () => {
    // Full at the top, nothing from the middle down, half way between.
    expect(linearCoverage([0.5, 0], [0.5, 0.5], [0.1, 0], 400, 200)).toBe(1);
    expect(linearCoverage([0.5, 0], [0.5, 0.5], [0.9, 0.75], 400, 200)).toBe(0);
    expect(linearCoverage([0.5, 0], [0.5, 0.5], [0.3, 0.25], 400, 200)).toBeCloseTo(0.5, 9);
  });
});
