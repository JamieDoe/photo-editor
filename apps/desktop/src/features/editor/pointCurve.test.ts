import { describe, expect, it } from "vitest";
import {
  DIAGONAL,
  MAX_POINTS,
  curvePath,
  evalCurve,
  insertPoint,
  isDiagonal,
  movePoint,
  nearestPoint,
  removePoint,
  resetEnd,
  type CurvePoint,
} from "./pointCurve";

const S: CurvePoint[] = [
  [0, 0],
  [0.25, 0.18],
  [0.75, 0.84],
  [1, 1],
];

describe("tone curve", () => {
  it("evaluates like the renderer", () => {
    // The same case as `matches_the_ui` in crates/renderer/src/ops/point_curve.rs.
    expect(evalCurve(S, 0.1)).toBeCloseTo(0.067625, 5);
    expect(evalCurve(S, 0.5)).toBeCloseTo(0.514515, 5);
    expect(evalCurve(S, 0.9)).toBeCloseTo(0.940561, 5);
  });

  it("is the diagonal by default, flat beyond its ends", () => {
    expect(isDiagonal(DIAGONAL)).toBe(true);
    expect(evalCurve(DIAGONAL, 0.37)).toBeCloseTo(0.37, 9);
    expect(isDiagonal(S)).toBe(false);
    const faded: CurvePoint[] = [
      [0.1, 0.08],
      [0.9, 0.95],
    ];
    expect(evalCurve(faded, 0)).toBe(0.08);
    expect(evalCurve(faded, 1)).toBe(0.95);
    expect(curvePath(DIAGONAL, 288, 120, 2)).toBe("M0.00 120.00 L144.00 60.00 L288.00 0.00");
  });

  it("adds points on the curve, in order, with room", () => {
    const added = insertPoint(S, 0.5)!;
    expect(added.index).toBe(2);
    expect(added.points[2]![1]).toBeCloseTo(evalCurve(S, 0.5), 4);
    // Too close to an existing point, or full.
    expect(insertPoint(S, 0.255)).toBeNull();
    const full = Array.from({ length: MAX_POINTS }, (_, i): CurvePoint => [i / (MAX_POINTS - 1), i / (MAX_POINTS - 1)]);
    expect(insertPoint(full, 0.03)).toBeNull();
  });

  it("moves points between their neighbours, within the graph", () => {
    const moved = movePoint(S, 1, 0.9, 1.4);
    expect(moved[1]).toEqual([0.74, 1]);
    expect(movePoint(S, 0, -0.2, 0.1)[0]).toEqual([0, 0.1]);
    expect(movePoint(S, 3, 0.5, 0.9)[3]).toEqual([0.76, 0.9]);
    // Rounded as the renderer keeps them.
    expect(movePoint(S, 1, 0.3333333, 0.2)[1]).toEqual([0.3333, 0.2]);
  });

  it("removes inner points and resets the ends", () => {
    expect(removePoint(S, 1)).toEqual([S[0], S[2], S[3]]);
    expect(removePoint(S, 0)).toEqual(S);
    expect(resetEnd(movePoint(S, 0, 0.1, 0.2), 0)[0]).toEqual([0, 0]);
  });

  it("finds the point under the pointer", () => {
    expect(nearestPoint(S, 0.26, 0.19, 0.03, 0.05)).toBe(1);
    expect(nearestPoint(S, 0.5, 0.5, 0.03, 0.05)).toBeNull();
  });
});
