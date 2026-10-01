import type { Calibration } from "../../ipc/generated/Calibration";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";

export type CalibrationKey = keyof Calibration;

const NEUTRAL: Calibration = {
  shadowTint: 0,
  redHue: 0,
  redSaturation: 0,
  greenHue: 0,
  greenSaturation: 0,
  blueHue: 0,
  blueSaturation: 0,
};

export function isCalibrationKey(key: string): key is CalibrationKey {
  return key in NEUTRAL;
}

/** The recipe's calibration (ADR 0053), or its neutral form. */
export function calibrationOf(r: EditRecipe): Calibration {
  return r.calibration ?? NEUTRAL;
}

export function calibrationEdited(r: EditRecipe): boolean {
  const c = r.calibration;
  return c !== undefined && Object.values(c).some((v) => v !== 0);
}

/** `r` with one slider changed; left out while all are 0, as the renderer writes it. */
export function withCalibration(r: EditRecipe, key: CalibrationKey, value: number): EditRecipe {
  const c = { ...calibrationOf(r), [key]: value };
  const { calibration: _, ...rest } = r;
  return Object.values(c).some((v) => v !== 0) ? { ...rest, calibration: c } : rest;
}

const grey = "rgba(160,160,165,0.3)";
const track = (from: string, to: string) => `linear-gradient(90deg, ${from}, ${grey} 50%, ${to})`;

/** Each slider's colours, as on Lightroom's: where it moves the colour, either way. */
export const CALIBRATION_TRACKS: Readonly<Record<CalibrationKey, string>> = {
  shadowTint: track("rgba(110,184,120,0.8)", "rgba(200,112,186,0.8)"),
  redHue: track("rgba(214,84,150,0.85)", "rgba(226,140,70,0.85)"),
  redSaturation: track("rgba(160,160,165,0.5)", "rgba(222,84,74,0.9)"),
  greenHue: track("rgba(196,200,80,0.85)", "rgba(70,190,170,0.85)"),
  greenSaturation: track("rgba(160,160,165,0.5)", "rgba(96,186,96,0.9)"),
  blueHue: track("rgba(80,180,200,0.85)", "rgba(140,96,214,0.85)"),
  blueSaturation: track("rgba(160,160,165,0.5)", "rgba(80,124,222,0.9)"),
};
