import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { ColourMixer } from "../../ipc/generated/ColourMixer";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { HslShift } from "../../ipc/generated/HslShift";

/** Recipe fields set with sliders (the look is a choice, and the mixer has its own
 *  controls). */
export type AdjustmentKey = Exclude<keyof EditRecipe, "version" | "look" | "mixer">;

const MIXER_BANDS: readonly (keyof ColourMixer)[] = [
  "red",
  "orange",
  "yellow",
  "green",
  "aqua",
  "blue",
  "purple",
  "magenta",
];
const MIXER_CONTROLS: readonly (keyof HslShift)[] = ["hue", "saturation", "luminance"];

export function isMixerBand(key: string): key is keyof ColourMixer {
  return (MIXER_BANDS as readonly string[]).includes(key);
}

export function isMixerControl(key: string): key is keyof HslShift {
  return (MIXER_CONTROLS as readonly string[]).includes(key);
}

export function mixerBandEdited(shift: HslShift): boolean {
  return MIXER_CONTROLS.some((k) => shift[k] !== 0);
}

/** The recipe's colour mixer; an absent mixer (the recipe never used it) is neutral. */
export function mixerOf(r: EditRecipe): ColourMixer {
  if (r.mixer) return r.mixer;
  const neutral = { hue: 0, saturation: 0, luminance: 0 };
  return Object.fromEntries(MIXER_BANDS.map((b) => [b, { ...neutral }])) as ColourMixer;
}

export function mixerEdited(r: EditRecipe): boolean {
  return r.mixer !== undefined && MIXER_BANDS.some((b) => mixerBandEdited(mixerOf(r)[b]));
}

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
  "texture",
  "clarity",
  "sharpening",
];

export function isAdjustmentKey(key: string): key is AdjustmentKey {
  return (ADJUSTMENT_KEYS as readonly string[]).includes(key);
}

/** Whether `r` is the default: every adjustment at its default, on the default look. */
export function isIdentity(r: EditRecipe): boolean {
  const defaults = neutralRecipe(r.version);
  return ADJUSTMENT_KEYS.every((k) => r[k] === defaults[k]) && !mixerEdited(r) && r.look === "standard";
}

/**
 * The recipe of an unedited photo: every adjustment at its default (zero, except the
 * design's default capture sharpening of 40) on the Standard look. The one place new
 * fields are added; defaults must match the engine's specs (crates/renderer/src/adjustments.rs).
 */
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
    texture: 0,
    clarity: 0,
    sharpening: 40,
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
