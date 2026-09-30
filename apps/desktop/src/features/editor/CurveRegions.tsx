import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { isRegionKey, parametricOf, withRegion } from "./pointCurve";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";

/**
 * The parametric tone curve's region sliders (ADR 0051), under the point curve as in
 * Lightroom: each lifts or lowers its part of the tonal range, before the points.
 */
export function CurveRegions({
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
  const curve = parametricOf(recipe);
  return (
    <div className="curve-regions">
      <div className="more-title">Regions</div>
      {specs.map((spec) => {
        const key = spec.key;
        if (!isRegionKey(key)) return null;
        return (
          <Slider
            key={key}
            id={`curve-region-${key}`}
            spec={spec}
            value={curve[key]}
            shown={formatSliderValue(curve[key], spec.min, spec.step, spec.unit)}
            disabled={disabled}
            onChange={(v) => onChange(withRegion(recipe, key, v))}
          />
        );
      })}
    </div>
  );
}
