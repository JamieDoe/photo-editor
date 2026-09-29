import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";

/** Recipe fields set with sliders (the look is a choice, not a slider). */
export type AdjustmentKey = Exclude<keyof EditRecipe, "version" | "look">;

const ADJUSTMENT_KEYS: readonly AdjustmentKey[] = [
  "exposure",
  "contrast",
  "highlights",
  "shadows",
  "whites",
  "blacks",
  "temperature",
  "tint",
  "vibrance",
  "saturation",
];

export function isAdjustmentKey(key: string): key is AdjustmentKey {
  return (ADJUSTMENT_KEYS as readonly string[]).includes(key);
}

/** Whether `r` is the default: every adjustment neutral, on the default look. */
export function isIdentity(r: EditRecipe): boolean {
  return ADJUSTMENT_KEYS.every((k) => r[k] === 0) && r.look === "standard";
}

/** Every adjustment at zero on the Standard look: the one place new fields are added. */
export function neutralRecipe(recipeVersion: number): EditRecipe {
  return {
    version: recipeVersion,
    exposure: 0,
    contrast: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    temperature: 0,
    tint: 0,
    vibrance: 0,
    saturation: 0,
    look: "standard",
  };
}

/** Builds the neutral recipe from engine-provided defaults (ranges live in Rust). */
export function defaultRecipe(recipeVersion: number, specs: AdjustmentSpec[]): EditRecipe {
  const recipe = neutralRecipe(recipeVersion);
  for (const spec of specs) {
    if (isAdjustmentKey(spec.key)) recipe[spec.key] = spec.default;
  }
  return recipe;
}
