import { describe, expect, it } from "vitest";
import { edgeRings } from "./brushTint";

/** The stacked opacity at distance `d` from a stroke drawn with `rings`. */
function stacked(rings: ReturnType<typeof edgeRings>, d: number): number {
  return rings.filter((r) => r.halfWidth >= d).reduce((c, r) => c + (1 - c) * r.opacity, 0);
}

/** The renderer's profile (ADR 0042). */
function profile(d: number, r: number, feather: number): number {
  const inner = r * (1 - feather / 100);
  if (d <= inner) return 1;
  if (d >= r) return 0;
  const t = (d - inner) / (r - inner);
  return 1 - t * t * (3 - 2 * t);
}

describe("brush overlay", () => {
  it("stacks rings into the renderer's soft edge", () => {
    for (const feather of [20, 50, 100]) {
      const r = 60;
      const rings = edgeRings(r, feather);
      // Full in the middle (at Feather 100 there is no solid core: all but), nothing
      // outside.
      expect(stacked(rings, 0)).toBeGreaterThan(0.99);
      expect(stacked(rings, r + 1)).toBe(0);
      // Within 0.08 of the profile across the soft edge (half a step at its steepest).
      for (let d = 0; d <= r; d += 3) {
        expect(Math.abs(stacked(rings, d) - profile(d, r, feather))).toBeLessThan(0.08);
      }
    }
  });

  it("draws a hard brush as one path", () => {
    expect(edgeRings(40, 0)).toEqual([{ halfWidth: 40, opacity: 1 }]);
  });
});
