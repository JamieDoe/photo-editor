import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import { sliderTrack } from "./sliderTrack";

/**
 * One adjustment slider as in the design: label, value (which reads “Reset” on hover
 * once edited), and a track filled from the neutral point. Double-click resets.
 */
export function Slider({
  id,
  spec,
  value,
  shown,
  disabled,
  onChange,
  track,
  zeroMark = true,
}: {
  id: string;
  spec: AdjustmentSpec;
  value: number;
  shown: string;
  disabled: boolean;
  onChange: (v: number) => void;
  /** A custom track (white balance colours) replaces the fill and the zero mark. */
  track?: string;
  zeroMark?: boolean;
}) {
  const edited = value !== spec.default;
  const fill = sliderTrack(value, spec.min, spec.max);
  const zero = track === undefined && zeroMark ? fill.zeroPercent : null;
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
        {zero !== null && <span className="slider-zero" style={{ left: `${zero}%` }} />}
        <input
          id={id}
          className="range"
          type="range"
          min={spec.min}
          max={spec.max}
          step={spec.step}
          value={value}
          disabled={disabled}
          style={{ background: track ?? fill.background }}
          onDoubleClick={() => onChange(spec.default)}
          onChange={(e) => onChange(Number(e.currentTarget.value))}
        />
      </div>
    </div>
  );
}
