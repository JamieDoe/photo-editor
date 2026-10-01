import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { CALIBRATION_TRACKS, calibrationOf, isCalibrationKey, withCalibration } from "./calibration";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";

/**
 * Calibration (ADR 0053): the shadows' tint, then each primary's Hue and Saturation,
 * under a heading per group as the engine gives them.
 */
export function CalibrationControls({
  specs,
  recipe,
  onChange,
  disabled,
}: {
  specs: AdjustmentSpec[];
  recipe: EditRecipe;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
}) {
  const calibration = calibrationOf(recipe);
  const groups = [...new Set(specs.map((s) => s.group))];
  return (
    <div className="calibration">
      {groups.map((group) => (
        <div key={group} className="calibration-group">
          <h4 className="calibration-heading">{group}</h4>
          {specs
            .filter((s) => s.group === group)
            .map((spec) => {
              const key = spec.key;
              if (!isCalibrationKey(key)) return null;
              const value = calibration[key];
              return (
                <Slider
                  key={key}
                  id={`calibration-${key}`}
                  spec={spec}
                  value={value}
                  shown={formatSliderValue(value, spec.min, spec.step, spec.unit)}
                  track={CALIBRATION_TRACKS[key]}
                  disabled={disabled}
                  onChange={(v) => onChange(withCalibration(recipe, key, v))}
                />
              );
            })}
        </div>
      ))}
    </div>
  );
}
