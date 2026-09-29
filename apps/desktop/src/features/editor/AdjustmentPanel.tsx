import type { ReactNode } from "react";
import { ColourIcon, DetailIcon, LightIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { PanelSection } from "./PanelSection";
import { isAdjustmentKey } from "./recipe";
import { formatSliderValue, sliderTrack } from "./sliderTrack";

interface Props {
  specs: AdjustmentSpec[];
  recipe: EditRecipe;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
}

const GROUP_ICONS: Record<string, ReactNode> = {
  Light: <LightIcon />,
  Colour: <ColourIcon />,
};

/**
 * Sliders generated from engine-provided specs, one collapsible section per group.
 * An edited value can be reset by clicking it (it reads “Reset” on hover) or by
 * double-clicking the slider.
 */
export function AdjustmentPanel({ specs, recipe, onChange, disabled }: Props) {
  const groups = [...new Set(specs.map((s) => s.group))];
  const valueOf = (spec: AdjustmentSpec) => (isAdjustmentKey(spec.key) ? recipe[spec.key] : spec.default);
  return (
    <>
      {groups.map((group) => {
        const groupSpecs = specs.filter((s) => s.group === group && isAdjustmentKey(s.key));
        const edited = groupSpecs.some((s) => valueOf(s) !== s.default);
        return (
          <PanelSection key={group} title={group} icon={GROUP_ICONS[group] ?? <DetailIcon />} edited={edited}>
            {groupSpecs.map((spec) => (
              <Slider
                key={spec.key}
                spec={spec}
                value={valueOf(spec)}
                disabled={disabled}
                onChange={(v) => {
                  if (isAdjustmentKey(spec.key)) onChange({ ...recipe, [spec.key]: v });
                }}
              />
            ))}
          </PanelSection>
        );
      })}
    </>
  );
}

function Slider({
  spec,
  value,
  disabled,
  onChange,
}: {
  spec: AdjustmentSpec;
  value: number;
  disabled: boolean;
  onChange: (v: number) => void;
}) {
  const id = `adjust-${spec.key}`;
  const edited = value !== spec.default;
  const track = sliderTrack(value, spec.min, spec.max);
  const shown = formatSliderValue(value, spec.min, spec.step);
  return (
    <div className={edited ? "slider edited" : "slider"}>
      <div className="slider-head">
        <label htmlFor={id}>{spec.label}</label>
        {edited ? (
          <button
            className="slider-value"
            title={`Reset ${spec.label}`}
            aria-label={`${spec.label} ${shown}. Reset`}
            disabled={disabled}
            onClick={() => onChange(spec.default)}
          >
            <span className="value">{shown}</span>
            <span className="reset">Reset</span>
          </button>
        ) : (
          <span className="slider-value">{shown}</span>
        )}
      </div>
      <div className="slider-track">
        {track.zeroPercent !== null && <span className="slider-zero" style={{ left: `${track.zeroPercent}%` }} />}
        <input
          id={id}
          className="range"
          type="range"
          min={spec.min}
          max={spec.max}
          step={spec.step}
          value={value}
          disabled={disabled}
          style={{ background: track.background }}
          onDoubleClick={() => onChange(spec.default)}
          onChange={(e) => onChange(Number(e.currentTarget.value))}
        />
      </div>
    </div>
  );
}
