import { describe, expect, it } from "vitest";
import { gradingEdited, gradingOf, nudgeWheel, wheelAt, wheelPoint, withGrading, withWheel } from "./colourGrading";
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

  it("moves the point the way an arrow key points", () => {
    const at = (hue: number, saturation: number) => ({ hue, saturation, luminance: 0 });
    // From the centre, right is red (hue 0) and up is hue 90.
    expect(nudgeWheel(at(0, 0), 0.02, 0)).toEqual({ hue: 0, saturation: 2 });
    expect(nudgeWheel(at(0, 0), 0, 0.02)).toEqual({ hue: 90, saturation: 2 });
    // At the top, left moves toward hue 180 and down weakens it.
    expect(nudgeWheel(at(90, 50), -0.02, 0).hue).toBeGreaterThan(90);
    expect(nudgeWheel(at(90, 50), 0, -0.02)).toEqual({ hue: 90, saturation: 48 });
    // A step always moves it, whichever way and wherever it is.
    for (let hue = 0; hue < 360; hue += 7) {
      for (const saturation of [1, 13, 50, 99]) {
        for (const [dx, dy] of [[0.02, 0], [-0.02, 0], [0, 0.02], [0, -0.02]] as const) {
          const next = nudgeWheel(at(hue, saturation), dx, dy);
          expect(next.hue !== hue || next.saturation !== saturation).toBe(true);
        }
      }
    }
  });
});
