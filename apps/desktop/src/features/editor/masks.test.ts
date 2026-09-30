import { describe, expect, it } from "vitest";
import type { Mask } from "../../ipc/generated/Mask";
import {
  FULL_CROP,
  flipMasks,
  fromShown,
  linearCoverage,
  maskName,
  newLinearMask,
  setAdjustment,
  toShown,
  turnMasks,
  withMasks,
} from "./masks";
import { neutralRecipe } from "./recipe";

describe("masks", () => {
  it("adds numbered linear gradients over the shown picture", () => {
    const crop = { x: 0.2, y: 0.1, w: 0.6, h: 0.8 };
    const a = newLinearMask([], crop);
    expect(a.id).toBe(1);
    // In frame fractions, inside the crop.
    expect(a.shape.start[0]).toBeCloseTo(0.5);
    expect(a.shape.start[1]).toBeCloseTo(0.18);
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
    expect(cw.shape.start).toEqual([0.9, 0.2]);
    const back = turnMasks(turnMasks([m], 1), -1)[0]!;
    expect(back.shape).toEqual(m.shape);
    expect(flipMasks([m])[0]!.shape.start).toEqual([0.8, 0.1]);
  });

  it("keeps the recipe's masks tidy", () => {
    const r = neutralRecipe(17);
    const m = newLinearMask([], FULL_CROP);
    const one = withMasks(r, setAdjustment([m], 1, "exposure", -0.5));
    expect(one.masks?.[0]?.adjustments.exposure).toBe(-0.5);
    expect(withMasks(one, []).masks).toBeUndefined();
  });

  it("covers as the renderer does", () => {
    // Full at the top, nothing from the middle down, half way between.
    expect(linearCoverage([0.5, 0], [0.5, 0.5], [0.1, 0], 400, 200)).toBe(1);
    expect(linearCoverage([0.5, 0], [0.5, 0.5], [0.9, 0.75], 400, 200)).toBe(0);
    expect(linearCoverage([0.5, 0], [0.5, 0.5], [0.3, 0.25], 400, 200)).toBeCloseTo(0.5, 9);
  });
});
