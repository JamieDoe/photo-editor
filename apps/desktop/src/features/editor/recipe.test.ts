import { describe, expect, it } from "vitest";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import { defaultRecipe, geometryEdited, isIdentity, mixerEdited, mixerOf, neutralRecipe } from "./recipe";

const specs: AdjustmentSpec[] = [
  { key: "exposure", label: "Exposure", group: "Light", min: -5, max: 5, step: 0.01, default: 0, more: false, unit: "EV" },
];

describe("recipe helpers", () => {
  it("counts framing and lens corrections as edits", () => {
    const r = defaultRecipe(13, specs);
    const whole = { straighten: 0, crop: { x: 0, y: 0, w: 1, h: 1 }, aspect: "square" as const, vertical: 0, horizontal: 0, rotation: 0, flip: false };
    // An aspect choice alone changes nothing.
    expect(isIdentity({ ...r, geometry: whole })).toBe(true);
    for (const g of [{ ...whole, straighten: 1 }, { ...whole, vertical: 5 }, { ...whole, crop: { x: 0.1, y: 0, w: 0.9, h: 1 } }, { ...whole, rotation: 2 }, { ...whole, flip: true }]) {
      expect(geometryEdited({ ...r, geometry: g })).toBe(true);
      expect(isIdentity({ ...r, geometry: g })).toBe(false);
    }
    const lens = { ...r, chromaticAberration: { red: [0, 0] as [number, number], blue: [0, 0] as [number, number] } };
    expect(geometryEdited(lens)).toBe(true);
    expect(isIdentity(lens)).toBe(false);
  });

  it("defaults to the Standard look with neutral adjustments", () => {
    const r = defaultRecipe(2, specs);
    expect(r.look).toBe("standard");
    expect(isIdentity(r)).toBe(true);
  });

  it("counts the Flat look, or any adjustment, as an edit", () => {
    const r = defaultRecipe(2, specs);
    expect(isIdentity({ ...r, look: "flat" })).toBe(false);
    expect(isIdentity({ ...r, exposure: 0.1 })).toBe(false);
  });

  it("counts default sharpening as unedited, and none as an edit", () => {
    const r = neutralRecipe(7);
    expect(r.sharpening).toBe(40);
    expect(isIdentity(r)).toBe(true);
    expect(isIdentity({ ...r, sharpening: 0 })).toBe(false);
  });

  it("treats an absent or all-zero mixer as unused", () => {
    const r = defaultRecipe(5, specs);
    expect(r.mixer).toBeUndefined();
    const neutral = mixerOf(r);
    expect(neutral.blue).toEqual({ hue: 0, saturation: 0, luminance: 0 });
    expect(isIdentity({ ...r, mixer: neutral })).toBe(true);
    const edited = { ...r, mixer: { ...neutral, blue: { ...neutral.blue, luminance: -20 } } };
    expect(mixerEdited(edited)).toBe(true);
    expect(isIdentity(edited)).toBe(false);
  });
});
