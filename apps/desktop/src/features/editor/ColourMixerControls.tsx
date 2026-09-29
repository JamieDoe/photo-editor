import { useState } from "react";
import type { ColourMixer } from "../../ipc/generated/ColourMixer";
import type { MixerSpec } from "../../ipc/generated/MixerSpec";
import { isMixerBand, isMixerControl, mixerBandEdited } from "./recipe";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";

/** Dot colours from the design, by band key. */
const BAND_COLOURS: Readonly<Record<string, string>> = {
  red: "#e5675c",
  orange: "#e8955a",
  yellow: "#e2c65a",
  green: "#7bbf6a",
  aqua: "#5cc2c0",
  blue: "#5b8fe0",
  purple: "#9670e0",
  magenta: "#d067b8",
};

/**
 * The design's colour mixer: pick a colour range, then its Hue, Saturation and
 * Luminance (ADR 0025). Bands and ranges come from the engine.
 */
export function ColourMixerControls({
  spec,
  mixer,
  disabled,
  onChange,
}: {
  spec: MixerSpec;
  mixer: ColourMixer;
  disabled: boolean;
  onChange: (m: ColourMixer) => void;
}) {
  // The design opens on Blues.
  const [selected, setSelected] = useState("blue");
  const band = spec.bands.find((b) => b.key === selected) ?? spec.bands[0];
  if (!band || !isMixerBand(band.key)) return null;
  const bandKey = band.key;
  const shift = mixer[bandKey];
  return (
    <>
      <div className="mixer-head">
        <span className="mixer-title">Colour mixer</span>
        <span className="mixer-band">{band.label}</span>
      </div>
      <div
        className="mixer-dots"
        role="radiogroup"
        aria-label="Colour range"
        onKeyDown={(e) => {
          // Arrow keys move between ranges, as in any radio group.
          const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
          if (step === 0) return;
          e.preventDefault();
          const i = spec.bands.findIndex((b) => b.key === bandKey);
          const next = spec.bands[(i + step + spec.bands.length) % spec.bands.length];
          if (next) {
            setSelected(next.key);
            e.currentTarget.querySelector<HTMLButtonElement>(`[data-band="${next.key}"]`)?.focus();
          }
        }}
      >
        {spec.bands.map((b) => {
          const edited = isMixerBand(b.key) && mixerBandEdited(mixer[b.key]);
          return (
            <button
              key={b.key}
              data-band={b.key}
              role="radio"
              aria-checked={b.key === bandKey}
              tabIndex={b.key === bandKey ? 0 : -1}
              aria-label={edited ? `${b.label} (edited)` : b.label}
              title={b.label}
              className={edited ? "mixer-dot edited" : "mixer-dot"}
              onClick={() => setSelected(b.key)}
            >
              <span style={{ background: BAND_COLOURS[b.key] ?? "var(--text-3)" }} />
            </button>
          );
        })}
      </div>
      {spec.controls.map((control) => {
        if (!isMixerControl(control.key)) return null;
        const key = control.key;
        const value = shift[key];
        return (
          <Slider
            key={key}
            id={`mixer-${key}`}
            spec={control}
            value={value}
            shown={formatSliderValue(value, control.min, control.step, control.unit)}
            disabled={disabled}
            zeroMark={false}
            onChange={(v) => onChange({ ...mixer, [bandKey]: { ...shift, [key]: v } })}
          />
        );
      })}
    </>
  );
}
