import { describe, expect, it } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { calibrationEdited, calibrationOf, isCalibrationKey, withCalibration } from "./calibration";

const base = { version: 23, exposure: 0 } as EditRecipe;

describe("calibration", () => {
  it("is written only while a slider is set", () => {
    expect(calibrationEdited(base)).toBe(false);
    const set = withCalibration(base, "blueHue", -40);
    expect(set.calibration?.blueHue).toBe(-40);
    expect(calibrationEdited(set)).toBe(true);
    const cleared = withCalibration(set, "blueHue", 0);
    expect(cleared.calibration).toBeUndefined();
    expect(calibrationOf(cleared).redHue).toBe(0);
  });

  it("knows its keys", () => {
    expect(isCalibrationKey("shadowTint")).toBe(true);
    expect(isCalibrationKey("exposure")).toBe(false);
  });
});
