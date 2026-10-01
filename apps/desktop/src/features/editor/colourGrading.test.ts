import { describe, expect, it } from "vitest";
import { gradingEdited, gradingOf, wheelAt, wheelPoint, withGrading, withWheel } from "./colourGrading";
import { neutralRecipe } from "./recipe";

describe("colour grading", () => {
  const base = neutralRecipe(22);

  it("sets a range's wheel, and leaves grading out while nothing is set", () => {
    const toned = withWheel(base, "shadows", { hue: 210, saturation: 30 });
    expect(toned.colourGrading?.shadows).toEqual({ hue: 210, saturation: 30, luminance: 0 });
    expect(gradingEdited(toned)).toBe(true);
    expect("colourGrading" in withWheel(toned, "shadows", { saturation: 0 })).toBe(false);
    // Blending alone is no grading.
    expect("colourGrading" in withGrading(base, { blending: 80 })).toBe(false);
    expect(gradingOf(base).blending).toBe(50);
  });

  it("maps wheel points to hue and strength and back", () => {
    const p = wheelPoint({ hue: 90, saturation: 50, luminance: 0 });
    expect(p.x).toBeCloseTo(0, 9);
    expect(p.y).toBeCloseTo(0.5, 9);
    expect(wheelAt(p.x, p.y)).toEqual({ hue: 90, saturation: 50 });
    // Beyond the rim: full strength.
    expect(wheelAt(-2, 0)).toEqual({ hue: 180, saturation: 100 });
  });
});
