import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { isAdjustmentKey } from "./recipe";

interface Props {
  specs: AdjustmentSpec[];
  recipe: EditRecipe;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
}

/** Sliders generated from engine-provided specs. Double-click a slider to reset it. */
export function AdjustmentPanel({ specs, recipe, onChange, disabled }: Props) {
  const groups = [...new Set(specs.map((s) => s.group))];
  return (
    <div className="adjustments">
      {groups.map((group) => (
        <section key={group}>
          <h3>{group}</h3>
          {specs
            .filter((s) => s.group === group)
            .map((spec) => {
              const key = spec.key;
              if (!isAdjustmentKey(key)) return null;
              const value = recipe[key];
              const set = (v: number) => onChange({ ...recipe, [key]: v });
              return (
                <label key={key} className="slider" onDoubleClick={() => set(spec.default)}>
                  <span className="slider-label">
                    {spec.label}
                    <output>{spec.step < 1 ? value.toFixed(2) : value.toFixed(0)}</output>
                  </span>
                  <input
                    type="range"
                    min={spec.min}
                    max={spec.max}
                    step={spec.step}
                    value={value}
                    disabled={disabled}
                    onChange={(e) => set(Number(e.currentTarget.value))}
                  />
                </label>
              );
            })}
        </section>
      ))}
    </div>
  );
}
