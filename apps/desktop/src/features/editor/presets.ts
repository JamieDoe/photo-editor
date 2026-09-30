import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PresetDto } from "../../ipc/generated/PresetDto";

/**
 * Presets in the editor (ADR 0046). A preset holds only a look; applying it keeps
 * what belongs to the photo. These fields mirror the renderer's
 * `EditRecipe::with_look_of` (crates/renderer/src/presets.rs).
 */
const PHOTO_FIELDS = ["exposure", "geometry", "chromaticAberration", "masks"] as const;

/** `current` with `preset`'s look: the preset's settings, keeping the photo's
 *  exposure, geometry, lens corrections and masks. */
export function applyPreset(current: EditRecipe, preset: PresetDto): EditRecipe {
  const next: EditRecipe = { ...preset.recipe, version: current.version };
  for (const k of PHOTO_FIELDS) {
    if (current[k] === undefined) delete next[k];
    else (next as Record<string, unknown>)[k] = current[k];
  }
  return next;
}

/** A recipe's look alone, as a canonical string (to compare looks). */
function lookKey(r: EditRecipe): string {
  const look: Record<string, unknown> = { ...r };
  for (const k of [...PHOTO_FIELDS, "version"]) delete look[k];
  return stableStringify(look);
}

/** Whether `current` has `preset`'s look (applied and not changed since). */
export function hasLook(current: EditRecipe, preset: PresetDto): boolean {
  return lookKey(current) === lookKey(preset.recipe);
}

/** The photo's own part of a recipe, as a canonical string: presets' previews only
 *  change when it does. */
export function photoKey(r: EditRecipe): string {
  return stableStringify(Object.fromEntries(PHOTO_FIELDS.map((k) => [k, r[k]])));
}

/** JSON with object keys sorted and undefined values left out. */
export function stableStringify(v: unknown): string {
  return JSON.stringify(v, (_, value: unknown) =>
    value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : value,
  );
}
