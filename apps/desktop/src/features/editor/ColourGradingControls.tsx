import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { GRADE_RANGES, gradingOf, wheelAt, wheelPoint, withGrading, withWheel, type GradeRange } from "./colourGrading";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";

/** The wheel's hues, in CSS conic-gradient order (clockwise from the top): a hue's
 *  direction is counter-clockwise from the right, as on Lightroom's wheels. */
const WHEEL_GRADIENT = `conic-gradient(${Array.from({ length: 13 }, (_, i) => `hsl(${(90 - i * 30 + 360) % 360} 85% 55%)`).join(", ")})`;

/**
 * Colour grading (ADR 0052): a range (Shadows, Midtones, Highlights or Global), its
 * wheel (drag the point: direction is the hue, distance from the centre the strength;
 * double-click to clear) and its Luminance, then the ranges' Blending and Balance.
 */
export function ColourGradingControls({
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
  const [range, setRange] = useState<GradeRange>("shadows");
  const grading = gradingOf(recipe);
  const wheel = grading[range];
  const spec = (key: string) => specs.find((s) => s.key === key);
  const luminance = spec("luminance");
  const blending = spec("blending");
  const balance = spec("balance");
  const discRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  const setFromPointer = (e: ReactPointerEvent) => {
    const box = discRef.current?.getBoundingClientRect();
    if (!box) return;
    const x = ((e.clientX - box.left) / box.width) * 2 - 1;
    const y = 1 - ((e.clientY - box.top) / box.height) * 2;
    onChange(withWheel(recipe, range, wheelAt(x, y)));
  };
  const point = wheelPoint(wheel);
  const label = GRADE_RANGES.find((r) => r.id === range)?.label ?? "";

  return (
    <div className="grading">
      <div className="segmented small grading-ranges" role="radiogroup" aria-label="Range">
        {GRADE_RANGES.map((r) => {
          const set = grading[r.id].saturation !== 0 || grading[r.id].luminance !== 0;
          return (
            <button key={r.id} role="radio" aria-checked={range === r.id} disabled={disabled} onClick={() => setRange(r.id)}>
              {r.label}
              {set && <span className="grading-dot" aria-hidden="true" />}
            </button>
          );
        })}
      </div>
      <div className="grading-wheel-row">
        <div
          ref={discRef}
          className="grading-wheel"
          style={{ backgroundImage: `radial-gradient(circle, rgb(128 128 128) 0%, rgb(128 128 128 / 0%) 72%), ${WHEEL_GRADIENT}` }}
          role="slider"
          aria-label={`${label} hue and strength`}
          aria-valuetext={`Hue ${wheel.hue}°, strength ${wheel.saturation}`}
          aria-valuenow={wheel.saturation}
          aria-valuemin={0}
          aria-valuemax={100}
          tabIndex={disabled ? -1 : 0}
          onPointerDown={(e) => {
            if (disabled) return;
            e.currentTarget.setPointerCapture(e.pointerId);
            dragging.current = true;
            setFromPointer(e);
          }}
          onPointerMove={(e) => dragging.current && setFromPointer(e)}
          onPointerUp={() => (dragging.current = false)}
          onPointerCancel={() => (dragging.current = false)}
          onDoubleClick={() => !disabled && onChange(withWheel(recipe, range, { hue: 0, saturation: 0 }))}
          onKeyDown={(e) => {
            // Arrows: left/right turn the hue, up/down change the strength.
            const step = e.shiftKey ? 10 : 1;
            const change =
              e.key === "ArrowLeft"
                ? { hue: (wheel.hue + step) % 360 }
                : e.key === "ArrowRight"
                  ? { hue: (wheel.hue - step + 360) % 360 }
                  : e.key === "ArrowUp"
                    ? { saturation: Math.min(100, wheel.saturation + step) }
                    : e.key === "ArrowDown"
                      ? { saturation: Math.max(0, wheel.saturation - step) }
                      : null;
            if (!change || disabled) return;
            e.preventDefault();
            onChange(withWheel(recipe, range, change));
          }}
        >
          <span
            className="grading-handle"
            style={{
              left: `${(point.x + 1) * 50}%`,
              top: `${(1 - point.y) * 50}%`,
              background: wheel.saturation > 0 ? `hsl(${wheel.hue} 85% 55%)` : "transparent",
            }}
          />
        </div>
        <div className="grading-readout">
          <span>
            <span className="grading-key">Hue</span> {wheel.saturation > 0 ? `${wheel.hue}°` : "—"}
          </span>
          <span>
            <span className="grading-key">Strength</span> {wheel.saturation}
          </span>
        </div>
      </div>
      {luminance && (
        <Slider
          id={`grading-${range}-luminance`}
          spec={luminance}
          value={wheel.luminance}
          shown={formatSliderValue(wheel.luminance, luminance.min, luminance.step, luminance.unit)}
          disabled={disabled}
          onChange={(v) => onChange(withWheel(recipe, range, { luminance: v }))}
        />
      )}
      {blending && (
        <Slider
          id="grading-blending"
          spec={blending}
          value={grading.blending}
          shown={formatSliderValue(grading.blending, blending.min, blending.step, blending.unit)}
          zeroMark={false}
          disabled={disabled}
          onChange={(v) => onChange(withGrading(recipe, { blending: v }))}
        />
      )}
      {balance && (
        <Slider
          id="grading-balance"
          spec={balance}
          value={grading.balance}
          shown={formatSliderValue(grading.balance, balance.min, balance.step, balance.unit)}
          disabled={disabled}
          onChange={(v) => onChange(withGrading(recipe, { balance: v }))}
        />
      )}
    </div>
  );
}
