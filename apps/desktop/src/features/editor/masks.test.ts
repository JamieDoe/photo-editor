import { describe, expect, it } from "vitest";
import type { Mask } from "../../ipc/generated/Mask";
import {
  FULL_CROP,
  flipMasks,
  fromShown,
  linearCoverage,
  maskName,
  brushSizeFromSlider,
  addShape,
  newMask,
  newShape,
  removeShape,
  setShapeMode,
  shapesOf,
  sliderFromBrushSize,
  withShapeAt,
  radialCoverage,
  type Point,
  setAdjustment,
  toShown,
  turnMasks,
  withMasks,
  addableKinds,
  generatedIn,
  generatedShape,
  maskOf,
  renameGenerated,
} from "./masks";
import { neutralRecipe } from "./recipe";

/** A linear shape's start point (the tests make linear masks). */
const startOf = (m: Mask) => (m.shape.kind === "linear" ? m.shape.start : null);

describe("masks", () => {
  it("adds numbered linear gradients over the shown picture", () => {
    const crop = { x: 0.2, y: 0.1, w: 0.6, h: 0.8 };
    const a = newMask("linear", [], crop);
    expect(a.id).toBe(1);
    // In frame fractions, inside the crop.
    expect(startOf(a)![0]).toBeCloseTo(0.5);
    expect(startOf(a)![1]).toBeCloseTo(0.18);
    const b = newMask("linear", [a], crop);
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
    const m = newMask("linear", [], FULL_CROP);
    const one = withMasks(r, setAdjustment([m], 1, "exposure", -0.5));
    expect(one.masks?.[0]?.adjustments.exposure).toBe(-0.5);
    expect(withMasks(one, []).masks).toBeUndefined();
  });

  it("adds radial gradients and turns them with the picture", () => {
    const m = newMask("radial", [], FULL_CROP);
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
    const m = newMask("brush", [], FULL_CROP);
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

  it("combines shapes in a mask, in order", () => {
    const m = newMask("linear", [], FULL_CROP);
    const disc = newShape("radial", FULL_CROP);
    const brush = newShape("brush", FULL_CROP);
    const two = addShape(addShape(m, "subtract", disc), "add", brush);
    expect(shapesOf(two).map((s) => [s.shape.kind, s.mode])).toEqual([
      ["linear", null],
      ["radial", "subtract"],
      ["brush", "add"],
    ]);
    expect(shapesOf(setShapeMode(two, 1, "intersect"))[1]!.mode).toBe("intersect");
    const moved = withShapeAt(two, 1, { ...(disc as Extract<Mask["shape"], { kind: "radial" }>), feather: 10 });
    expect(moved.parts![0]!.shape).toMatchObject({ kind: "radial", feather: 10 });
    expect(withShapeAt(two, 0, disc).shape).toBe(disc);
    // Removing the first makes the next the first; the last one left stays.
    expect(shapesOf(removeShape(two, 0)).map((s) => s.shape.kind)).toEqual(["radial", "brush"]);
    expect(shapesOf(removeShape(two, 2)).map((s) => s.shape.kind)).toEqual(["linear", "radial"]);
    const single = removeShape(removeShape(two, 2), 1);
    expect(single.parts).toBeUndefined();
    expect(removeShape(single, 0)).toBe(single);
  });

  it("turns every shape of a mask with the picture", () => {
    const m: Mask = {
      id: 1,
      shape: { kind: "linear", start: [0.2, 0.1], end: [0.2, 0.6] },
      parts: [{ mode: "subtract", shape: { kind: "radial", centre: [0.2, 0.1], radius: [0.1, 0.1], angle: 0, feather: 50 } }],
      adjustments: { exposure: 0, warmth: 0, clarity: 0 },
    };
    const part = turnMasks([m], 1)[0]!.parts![0]!;
    expect(part.mode).toBe("subtract");
    expect(part.shape).toMatchObject({ centre: [0.9, 0.2], angle: 90 });
    expect(flipMasks([m])[0]!.parts![0]!.shape).toMatchObject({ centre: [0.8, 0.1] });
  });

  it("names generated masks by what they cover, and keeps them put when the picture turns", () => {
    const subject = maskOf(generatedShape("subject", "aa"), []);
    const linear = newMask("linear", [subject], FULL_CROP);
    const people = maskOf(generatedShape("people", "bb"), [subject, linear]);
    const masks = [subject, linear, people];
    expect(masks.map((m) => maskName(masks, m))).toEqual(["Subject", "Linear gradient", "People"]);
    // Made in the photo's own coordinates: the renderer maps them, so turns leave them.
    expect(turnMasks(masks, 1)[0]!.shape).toEqual(subject.shape);
    expect(flipMasks(masks)[2]!.shape).toEqual(people.shape);
  });

  it("offers generated masks only where they can be made", () => {
    expect(addableKinds([])).toEqual(["brush", "linear", "radial"]);
    expect(addableKinds(["subject", "people"])).toEqual(["subject", "people", "brush", "linear", "radial"]);
    expect(addableKinds(["subject"])).toEqual(["subject", "brush", "linear", "radial"]);
    // Every platform finds the sky; the design's order, People after it.
    expect(addableKinds(["sky"])).toEqual(["sky", "brush", "linear", "radial"]);
    expect(addableKinds(["people", "sky", "subject"])).toEqual(["subject", "sky", "people", "brush", "linear", "radial"]);
  });

  it("finds and renames the generated masks a recipe names", () => {
    const subject = maskOf(generatedShape("subject", "aa"), []);
    // A brush mask less the subject, and the same subject again.
    const brush = addShape(newMask("brush", [subject], FULL_CROP), "subtract", generatedShape("subject", "aa"));
    const people = maskOf(generatedShape("people", "bb"), [subject, brush]);
    const r = withMasks(neutralRecipe(26), [subject, brush, people]);
    expect(generatedIn(r)).toEqual([
      { name: "aa", kind: "subject" },
      { name: "bb", kind: "people" },
    ]);
    const renamed = renameGenerated(r, new Map([["aa", "cc"]]));
    expect(generatedIn(renamed)).toEqual([
      { name: "cc", kind: "subject" },
      { name: "bb", kind: "people" },
    ]);
    expect(shapesOf(renamed.masks![1]!)[1]!.shape).toEqual(generatedShape("subject", "cc"));
    // Everything else as it was.
    expect(renamed.masks![1]!.shape).toEqual(brush.shape);
  });
});
