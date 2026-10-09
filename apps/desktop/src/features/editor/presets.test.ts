import { describe, expect, it } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PresetDto } from "../../ipc/generated/PresetDto";
import { applyPreset, appliedPreset, hasLook, photoKey, stableStringify } from "./presets";
import { neutralRecipe } from "./recipe";

const mono: PresetDto = {
  id: "builtin:mono",
  name: "Mono",
  builtIn: true,
  recipe: { ...neutralRecipe(20), saturation: -100, contrast: 30, clarity: 20 },
};
const photo: EditRecipe = {
  ...neutralRecipe(20),
  exposure: 0.7,
  contrast: 45,
  vibrance: 20,
  masks: [{ id: 1, shape: { kind: "brush", strokes: [] }, adjustments: { exposure: -1, warmth: 0, clarity: 0 } }],
};

describe("presets", () => {
  it("replaces the look and keeps the photo's own settings", () => {
    const r = applyPreset(photo, mono);
    expect([r.saturation, r.contrast, r.clarity, r.vibrance]).toEqual([-100, 30, 20, 0]);
    expect(r.exposure).toBe(0.7);
    expect(r.masks).toBe(photo.masks);
    // Fields the photo does not have are left out, not set to undefined.
    expect("geometry" in r).toBe(false);
  });

  it("keeps the photo's lens corrections, which belong to its lens", () => {
    const off = applyPreset({ ...photo, profileCorrections: false }, { ...mono, recipe: { ...mono.recipe, profileCorrections: true } });
    expect(off.profileCorrections).toBe(false);
    expect("profileCorrections" in applyPreset(photo, mono)).toBe(false);
  });

  it("knows when a photo has a preset's look", () => {
    const r = applyPreset(photo, mono);
    expect(hasLook(r, mono)).toBe(true);
    expect(hasLook({ ...r, exposure: 2 }, mono)).toBe(true);
    expect(hasLook({ ...r, clarity: 21 }, mono)).toBe(false);
    expect(hasLook(photo, mono)).toBe(false);
  });

  it("keys previews by the photo's own settings only", () => {
    expect(photoKey({ ...photo, contrast: 0 })).toBe(photoKey(photo));
    expect(photoKey({ ...photo, exposure: 0 })).not.toBe(photoKey(photo));
    expect(stableStringify({ b: 1, a: { d: 2, c: undefined } })).toBe('{"a":{"d":2},"b":1}');
  });

  it("shows one preset as applied when several share a look", () => {
    const natural: PresetDto = { id: "builtin:natural", name: "Natural", builtIn: true, recipe: { ...neutralRecipe(20), vibrance: 12 } };
    const mine: PresetDto = { ...natural, id: "user:1", name: "Mine", builtIn: false };
    const all = [natural, mono, mine];
    const r = applyPreset(photo, natural);
    // The one chosen, while the photo has its look.
    expect(appliedPreset(r, all, "builtin:natural")).toBe("builtin:natural");
    expect(appliedPreset(r, all, "user:1")).toBe("user:1");
    // Nothing chosen (or the choice no longer matches): the saved preset first.
    expect(appliedPreset(r, all, undefined)).toBe("user:1");
    expect(appliedPreset(r, all, "builtin:mono")).toBe("user:1");
    expect(appliedPreset(r, [natural, mono], undefined)).toBe("builtin:natural");
    expect(appliedPreset(photo, all, "builtin:natural")).toBeNull();
  });
});
