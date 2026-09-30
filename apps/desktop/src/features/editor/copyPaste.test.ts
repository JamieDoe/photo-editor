import { describe, expect, it } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { SettingGroup } from "../../ipc/generated/SettingGroup";
import { defaultCopyGroups, pasteChanges, pasteEdits } from "./copyPaste";
import { neutralRecipe } from "./recipe";

// As the engine sends them (crates/renderer/src/settings.rs).
const groups: SettingGroup[] = [
  { id: "exposure", label: "Exposure", fields: ["exposure"], copiedByDefault: true },
  { id: "light", label: "Light and tone curve", fields: ["look", "contrast", "pointCurve"], copiedByDefault: true },
  { id: "whiteBalance", label: "White balance", fields: ["temperature", "tint"], copiedByDefault: true },
  { id: "geometry", label: "Crop, geometry and lens", fields: ["geometry", "chromaticAberration"], copiedByDefault: false },
  { id: "masks", label: "Masks", fields: ["masks"], copiedByDefault: false },
];
const masks: EditRecipe["masks"] = [{ id: 1, shape: { kind: "brush", strokes: [] }, adjustments: { exposure: 1, warmth: 0, clarity: 0 } }];
const source: EditRecipe = {
  ...neutralRecipe(20),
  exposure: 0.8,
  contrast: 25,
  temperature: 12,
  pointCurve: [
    [0, 0.1],
    [1, 1],
  ],
  masks,
};
const target: EditRecipe = {
  ...neutralRecipe(20),
  exposure: -0.3,
  contrast: -10,
  tint: 7,
  geometry: { straighten: 2, crop: { x: 0, y: 0, w: 1, h: 1 }, aspect: "original", vertical: 0, horizontal: 0, rotation: 0, flip: false },
};

describe("copy and paste", () => {
  it("copies the default groups, leaving the crop and masks", () => {
    expect(defaultCopyGroups(groups)).toEqual(["exposure", "light", "whiteBalance"]);
    const r = pasteEdits(target, { recipe: source, groups: defaultCopyGroups(groups) }, groups);
    expect([r.exposure, r.contrast, r.temperature, r.tint]).toEqual([0.8, 25, 12, 0]);
    expect(r.pointCurve).toEqual(source.pointCurve);
    // The target keeps its own crop, and gets no masks.
    expect(r.geometry).toBe(target.geometry);
    expect(r.masks).toBeUndefined();
  });

  it("pastes only the groups chosen, and removes what the source does not have", () => {
    const onlyGeometry = pasteEdits(target, { recipe: source, groups: ["geometry", "masks"] }, groups);
    // The source has no crop: pasting geometry takes the target's away.
    expect("geometry" in onlyGeometry).toBe(false);
    expect(onlyGeometry.masks).toBe(masks);
    expect(onlyGeometry.exposure).toBe(-0.3);
  });

  it("knows when pasting would change nothing", () => {
    const copied = { recipe: source, groups: ["exposure"] };
    expect(pasteChanges(target, copied, groups)).toBe(true);
    expect(pasteChanges(pasteEdits(target, copied, groups), copied, groups)).toBe(false);
  });
});
