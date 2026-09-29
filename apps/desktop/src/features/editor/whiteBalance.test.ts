import { describe, expect, it } from "vitest";
import { formatKelvin, kelvinAt } from "./whiteBalance";

// The same numbers as `temperature_scale_matches_the_rendered_light` in
// crates/renderer/src/ops/white_balance.rs.
const scale = { asShotKelvin: 5000, miredPerUnit: 1.2, minKelvin: 1667, maxKelvin: 25000 };

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
