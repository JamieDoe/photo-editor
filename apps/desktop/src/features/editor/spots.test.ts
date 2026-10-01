import { describe, expect, it } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Spot } from "../../ipc/generated/Spot";
import { frameToSource, moveSpot, sourceToFrame, spotAt, spotsOf, stillToFix, withSpots, withoutSpots } from "./spots";

const flat = { straighten: 0, vertical: 0, horizontal: 0, rotation: 0, flip: false };
const spot = (x: number, y: number, radius = 0.05): Spot => ({ kind: "heal", x, y, sourceX: x + 0.2, sourceY: y, radius, feather: 30, opacity: 100 });

describe("spots", () => {
  it("are left out of the recipe when there are none", () => {
    const r = { version: 24 } as EditRecipe;
    expect(spotsOf(r)).toEqual([]);
    const set = withSpots(r, [spot(0.3, 0.3)]);
    expect(spotsOf(set)).toHaveLength(1);
    expect(withSpots(set, []).spots).toBeUndefined();
  });

  it("map the frame to the source as the renderer does", () => {
    const id = frameToSource(flat, 300, 200);
    expect(id([0.25, 0.75])).toEqual([0.25, 0.75]);
    // Turned a quarter clockwise: the turned photo's top left is the source's bottom left.
    const turned = frameToSource({ ...flat, rotation: 1 }, 300, 200);
    const [x, y] = turned([0, 0]);
    expect(x).toBeCloseTo(0, 9);
    expect(y).toBeCloseTo(1, 9);
    // Mirrored: left and right swap.
    const [fx] = frameToSource({ ...flat, flip: true }, 300, 200)([0.2, 0.5]);
    expect(fx).toBeCloseTo(0.8, 9);
  });

  it("map back from the source, whatever the geometry", () => {
    for (const g of [
      flat,
      { ...flat, straighten: 7.5 },
      { ...flat, vertical: 40, horizontal: -25, straighten: -3 },
      { ...flat, rotation: 3, flip: true, straighten: 2 },
    ]) {
      const to = frameToSource(g, 600, 400);
      const back = sourceToFrame(g, 600, 400);
      for (const p of [[0.2, 0.3], [0.5, 0.5], [0.8, 0.9]] as const) {
        const [x, y] = to(back([p[0], p[1]]));
        expect(x).toBeCloseTo(p[0], 5);
        expect(y).toBeCloseTo(p[1], 5);
      }
    }
  });

  it("find the spot or source under a point", () => {
    const spots = [spot(0.3, 0.5), spot(0.32, 0.5)];
    // The later one wins where they overlap.
    expect(spotAt(spots, [0.31, 0.5], 1.5)).toBe(1);
    expect(spotAt(spots, [0.26, 0.5], 1.5)).toBe(0);
    expect(spotAt(spots, [0.9, 0.9], 1.5)).toBeNull();
    expect(spotAt(spots, [0.46, 0.5], 1.5, true)).toBe(0);
    // Round on a wide photo: 0.05 of the long edge is 0.075 of the height.
    expect(spotAt([spot(0.5, 0.5)], [0.5, 0.57], 1.5)).toBe(0);
    expect(spotAt([spot(0.5, 0.5)], [0.57, 0.5], 1.5)).toBeNull();
  });

  it("move a spot or its source", () => {
    const s = spot(0.3, 0.5);
    expect(moveSpot(s, [0.1, -0.1], "spot")).toMatchObject({ x: 0.4, y: 0.4, sourceX: 0.5 });
    expect(moveSpot(s, [0.6, 0], "source")).toMatchObject({ x: 0.3, sourceX: 1 });
  });

  it("know which dust is still to fix, and undo a Fix all", () => {
    const found = [spot(0.2, 0.2, 0.01), spot(0.6, 0.4, 0.01)];
    const byHand = spot(0.205, 0.2, 0.02);
    expect(stillToFix(found, [byHand], 1.5)).toEqual([found[1]]);
    expect(stillToFix(found, [], 1.5)).toEqual(found);
    const after = [byHand, ...found];
    expect(withoutSpots(after, found)).toEqual([byHand]);
  });
});
