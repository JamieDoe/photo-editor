/**
 * Track fill for an adjustment slider, as in the design: the filled part runs from the
 * neutral point (zero for bipolar ranges such as −100…100, otherwise the minimum) to the
 * current value.
 */
export interface SliderTrack {
  /** CSS background for the track. */
  background: string;
  /** Position of the zero mark in percent, for bipolar ranges; otherwise null. */
  zeroPercent: number | null;
}

export function sliderTrack(value: number, min: number, max: number): SliderTrack {
  const span = max - min || 1;
  const clamp = (p: number) => Math.min(100, Math.max(0, p));
  const pct = clamp(((value - min) / span) * 100);
  const bipolar = min < 0 && max > 0;
  const zero = bipolar ? clamp(((0 - min) / span) * 100) : 0;
  const a = Math.min(pct, zero).toFixed(2);
  const b = Math.max(pct, zero).toFixed(2);
  return {
    background: `linear-gradient(90deg, var(--track) ${a}%, var(--track-fill) ${a}%, var(--track-fill) ${b}%, var(--track) ${b}%)`,
    zeroPercent: bipolar ? zero : null,
  };
}

/** Value as shown beside the slider: signed for bipolar ranges, two decimals for fine
 *  steps, then the unit if any ("+0.50 EV", as in the design). */
export function formatSliderValue(value: number, min: number, step: number, unit = ""): string {
  const text = step < 1 ? value.toFixed(2) : String(Math.round(value));
  const signed = min < 0 && value > 0 ? `+${text}` : text;
  if (!unit) return signed;
  // Degrees attach to the number ("+1.4°", as in the design); other units are words.
  return unit === "°" ? `${signed}${unit}` : `${signed} ${unit}`;
}
