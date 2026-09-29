import { describe, expect, it } from "vitest";
import { neutralRecipe } from "./recipe";
import { toneCurveKey } from "./useToneCurve";

describe("toneCurveKey", () => {
  it("changes with Light controls and the look, not with colour or detail", () => {
    const r = neutralRecipe(8);
    const key = toneCurveKey(r);
    expect(toneCurveKey({ ...r, saturation: 40, clarity: 20, dehaze: 30 })).toBe(key);
    expect(toneCurveKey({ ...r, shadows: 10 })).not.toBe(key);
    expect(toneCurveKey({ ...r, look: "flat" })).not.toBe(key);
  });
});
