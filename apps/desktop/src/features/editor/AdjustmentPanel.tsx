import { useState, type ReactNode } from "react";
import { ChevronIcon, ColourIcon, DetailIcon, LightIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Look } from "../../ipc/generated/Look";
import type { TemperatureScale } from "../../ipc/generated/TemperatureScale";
import { PanelSection } from "./PanelSection";
import { isAdjustmentKey } from "./recipe";
import { formatSliderValue, sliderTrack } from "./sliderTrack";
import { formatKelvin, kelvinAt, WHITE_BALANCE_TRACKS } from "./whiteBalance";

interface Props {
  specs: AdjustmentSpec[];
  recipe: EditRecipe;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
  /** Shows Temperature in kelvin; null when the photo's as-shot light is unknown. */
  temperatureScale: TemperatureScale | null;
}

const LOOKS: ReadonlyArray<{ id: Look; label: string; hint: string }> = [
  { id: "standard", label: "Standard", hint: "Camera-like brightness and contrast (the default)" },
  { id: "flat", label: "Flat", hint: "No base tone curve: how photos looked before this update" },
];

const GROUP_ICONS: Record<string, ReactNode> = {
  Light: <LightIcon />,
  Colour: <ColourIcon />,
};

/**
 * Sliders generated from engine-provided specs, one collapsible section per group.
 * An edited value can be reset by clicking it (it reads “Reset” on hover) or by
 * double-clicking the slider.
 */
export function AdjustmentPanel({ specs, recipe, onChange, disabled, temperatureScale }: Props) {
  const groups = [...new Set(specs.map((s) => s.group))];
  const valueOf = (spec: AdjustmentSpec) => (isAdjustmentKey(spec.key) ? recipe[spec.key] : spec.default);
  // As in the design, Temperature reads as the light it assumes ("5650 K").
  const format = (spec: AdjustmentSpec, v: number) =>
    spec.key === "temperature" && temperatureScale
      ? formatKelvin(kelvinAt(temperatureScale, v))
      : formatSliderValue(v, spec.min, spec.step, spec.unit);
  return (
    <>
      <div className="look-row">
        <span className="look-label" id="look-label">
          Look
        </span>
        <div className="segmented small" role="radiogroup" aria-labelledby="look-label">
          {LOOKS.map((l) => (
            <label key={l.id} title={l.hint}>
              <input
                className="sr-only"
                type="radio"
                name="look"
                checked={recipe.look === l.id}
                disabled={disabled}
                onChange={() => onChange({ ...recipe, look: l.id })}
              />
              {l.label}
            </label>
          ))}
        </div>
      </div>
      {groups.map((group) => {
        const groupSpecs = specs.filter((s) => s.group === group && isAdjustmentKey(s.key));
        const edited = groupSpecs.some((s) => valueOf(s) !== s.default);
        return (
          <PanelSection key={group} title={group} icon={GROUP_ICONS[group] ?? <DetailIcon />} edited={edited}>
            <GroupSliders
              specs={groupSpecs}
              valueOf={valueOf}
              format={format}
              disabled={disabled}
              onChange={(key, v) => {
                if (isAdjustmentKey(key)) onChange({ ...recipe, [key]: v });
              }}
            />
          </PanelSection>
        );
      })}
    </>
  );
}

/**
 * A section's sliders: the everyday ones, then (as in the design) "More controls"
 * revealing the rest above a dashed divider. Opens by itself when a hidden slider
 * is already edited, so an edit is never out of sight.
 */
function GroupSliders({
  specs,
  valueOf,
  format,
  disabled,
  onChange,
}: {
  specs: AdjustmentSpec[];
  valueOf: (s: AdjustmentSpec) => number;
  format: (s: AdjustmentSpec, v: number) => string;
  disabled: boolean;
  onChange: (key: string, v: number) => void;
}) {
  const basic = specs.filter((s) => !s.more);
  const more = specs.filter((s) => s.more);
  const moreEdited = more.some((s) => valueOf(s) !== s.default);
  const [open, setOpen] = useState(false);
  const showMore = open || moreEdited;
  const slider = (spec: AdjustmentSpec) => (
    <Slider
      key={spec.key}
      spec={spec}
      value={valueOf(spec)}
      shown={format(spec, valueOf(spec))}
      disabled={disabled}
      onChange={(v) => onChange(spec.key, v)}
    />
  );
  return (
    <>
      {basic.map(slider)}
      {more.length > 0 && (
        <>
          {showMore && <div className="more-controls">{more.map(slider)}</div>}
          <button
            className="more-toggle"
            aria-expanded={showMore}
            disabled={moreEdited}
            title={moreEdited ? "Shown while one of these is edited" : undefined}
            onClick={() => setOpen(!open)}
          >
            <ChevronIcon size={12} />
            {showMore ? "Fewer controls" : "More controls"}
          </button>
        </>
      )}
    </>
  );
}

function Slider({
  spec,
  value,
  shown,
  disabled,
  onChange,
}: {
  spec: AdjustmentSpec;
  value: number;
  shown: string;
  disabled: boolean;
  onChange: (v: number) => void;
}) {
  const id = `adjust-${spec.key}`;
  const edited = value !== spec.default;
  const track = sliderTrack(value, spec.min, spec.max);
  // White balance sliders show their colours instead of a fill, as in the design.
  const background = WHITE_BALANCE_TRACKS[spec.key] ?? track.background;
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
          style={{ background }}
          onDoubleClick={() => onChange(spec.default)}
          onChange={(e) => onChange(Number(e.currentTarget.value))}
        />
      </div>
    </div>
  );
}
