import { describe, expect, it } from "vitest";
import { neutralRecipe } from "./recipe";
import { formatKelvin, kelvinAt, relativeWhiteBalance, withRelativeWhiteBalance } from "./whiteBalance";

// The same numbers as `temperature_scale_matches_the_rendered_light` in
// crates/renderer/src/ops/white_balance.rs.
const scale = { asShotKelvin: 5000, asShotTint: 0, miredPerUnit: 1.2, minKelvin: 1667, maxKelvin: 25000 };

describe("kelvinAt", () => {
  it("matches the renderer's light", () => {
    expect(kelvinAt(scale, 0)).toBe(5000);
    expect(kelvinAt(scale, 50)).toBeCloseTo(7142.857, 2);
    expect(kelvinAt(scale, -100)).toBeCloseTo(3125, 2);
  });

  it("stays within the supported range", () => {
    expect(kelvinAt({ ...scale, asShotKelvin: 20000 }, 100)).toBe(25000);
    expect(kelvinAt({ ...scale, asShotKelvin: 1800 }, -100)).toBe(1667);
  });

  it("formats as in the design", () => {
    expect(formatKelvin(5649.6)).toBe("5650 K");
  });
});

describe("white balance set as a light", () => {
  const scale = { asShotKelvin: 5000, asShotTint: 6, miredPerUnit: 1.2, minKelvin: 1667, maxKelvin: 25000 };
  const base = neutralRecipe(21);

  it("shows as the shift it amounts to for the photo", () => {
    // 5000 K as shot to 6250 K: 200 - 160 = 40 mired warmer, +33.33 on the slider.
    const r = { ...base, whiteBalance: { kelvin: 6250, tint: 16 } };
    expect(relativeWhiteBalance(r, scale)).toEqual({ temperature: 33.33, tint: 10 });
    expect(relativeWhiteBalance({ ...base, temperature: 12, tint: -3 }, scale)).toEqual({ temperature: 12, tint: -3 });
  });

  it("becomes that shift when a slider moves", () => {
    const r = withRelativeWhiteBalance({ ...base, whiteBalance: { kelvin: 6250, tint: 16 } }, scale);
    expect("whiteBalance" in r).toBe(false);
    expect([r.temperature, r.tint]).toEqual([33.33, 10]);
    // Far beyond the sliders' reach: they stop at their ends.
    expect(relativeWhiteBalance({ ...base, whiteBalance: { kelvin: 20000, tint: 150 } }, scale).tint).toBe(100);
  });
});
