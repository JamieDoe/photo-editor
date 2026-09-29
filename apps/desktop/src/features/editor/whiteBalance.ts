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
