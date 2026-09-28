import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";

export type AdjustmentKey = Exclude<keyof EditRecipe, "version">;

const ADJUSTMENT_KEYS: readonly AdjustmentKey[] = ["exposure", "contrast", "temperature", "saturation"];

export function isAdjustmentKey(key: string): key is AdjustmentKey {
  return (ADJUSTMENT_KEYS as readonly string[]).includes(key);
}

/** Builds the neutral recipe from engine-provided defaults (ranges live in Rust). */
export function defaultRecipe(recipeVersion: number, specs: AdjustmentSpec[]): EditRecipe {
  const recipe: EditRecipe = { version: recipeVersion, exposure: 0, contrast: 0, temperature: 0, saturation: 0 };
  for (const spec of specs) {
    if (isAdjustmentKey(spec.key)) recipe[spec.key] = spec.default;
  }
  return recipe;
}
