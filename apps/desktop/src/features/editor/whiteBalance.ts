import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { TemperatureScale } from "../../ipc/generated/TemperatureScale";

/**
 * The colour temperature a Temperature slider value assumes, for a photo whose
 * as-shot light is known (ADR 0024). Mirrors `TemperatureScale::kelvin_at` in Rust:
 * the slider moves the light in mired, relative to the as-shot light.
 */
export function kelvinAt(scale: TemperatureScale, amount: number): number {
  const mired = 1e6 / scale.asShotKelvin - amount * scale.miredPerUnit;
  const kelvin = 1e6 / Math.max(mired, 1);
  return Math.min(scale.maxKelvin, Math.max(scale.minKelvin, kelvin));
}

/** "5650 K", as in the design. */
export function formatKelvin(kelvin: number): string {
  return `${Math.round(kelvin)} K`;
}

/** Slider tracks for the white balance controls, as in the design. */
export const WHITE_BALANCE_TRACKS: Readonly<Record<string, string>> = {
  temperature: "linear-gradient(90deg, rgba(93,134,201,0.85), rgba(160,160,165,0.3) 50%, rgba(222,168,84,0.85))",
  tint: "linear-gradient(90deg, rgba(110,184,120,0.8), rgba(160,160,165,0.3) 50%, rgba(200,112,186,0.8))",
};

/** A display-referred photo's white: D65 (6504 K, a little green of the locus), as the
 *  renderer assumes without an as-shot light. */
const D65_SCALE: TemperatureScale = { asShotKelvin: 6504, asShotTint: 9.6, miredPerUnit: 1.2, minKelvin: 1667, maxKelvin: 25000 };

const clamp100 = (v: number) => Math.round(Math.min(100, Math.max(-100, v)) * 100) / 100;

/**
 * The Temperature and Tint slider values of `recipe` for this photo. A white balance
 * set as a light (ADR 0051) shows as the shift from the photo's as-shot light it
 * amounts to, as the renderer applies it.
 */
export function relativeWhiteBalance(recipe: EditRecipe, scale: TemperatureScale | null): { temperature: number; tint: number } {
  const light = recipe.whiteBalance;
  if (!light) return { temperature: recipe.temperature, tint: recipe.tint };
  const s = scale ?? D65_SCALE;
  return {
    temperature: clamp100((1e6 / s.asShotKelvin - 1e6 / light.kelvin) / s.miredPerUnit),
    tint: clamp100(light.tint - s.asShotTint),
  };
}

/** `recipe` with a white balance set as a light turned into the sliders' values, so a
 *  slider moves from where it shows. */
export function withRelativeWhiteBalance(recipe: EditRecipe, scale: TemperatureScale | null): EditRecipe {
  if (!recipe.whiteBalance) return recipe;
  const { whiteBalance: _, ...rest } = recipe;
  return { ...rest, ...relativeWhiteBalance(recipe, scale) };
}
