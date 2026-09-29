import { describe, expect, it } from "vitest";
import { formatSliderValue, sliderTrack } from "./sliderTrack";

describe("sliderTrack", () => {
  it("fills from zero to the value on bipolar ranges", () => {
    expect(sliderTrack(50, -100, 100)).toEqual({
      background:
        "linear-gradient(90deg, var(--track) 50.00%, var(--track-fill) 50.00%, var(--track-fill) 75.00%, var(--track) 75.00%)",
      zeroPercent: 50,
    });
    // Negative values fill leftwards from zero.
    expect(sliderTrack(-2.5, -5, 5).background).toContain("var(--track) 25.00%, var(--track-fill) 25.00%, var(--track-fill) 50.00%");
  });

  it("fills from the minimum on one-sided ranges", () => {
    const t = sliderTrack(75, 50, 100);
    expect(t.zeroPercent).toBeNull();
    expect(t.background).toContain("var(--track) 0.00%, var(--track-fill) 0.00%, var(--track-fill) 50.00%");
  });

  it("clamps out-of-range values", () => {
    expect(sliderTrack(500, -100, 100).background).toContain("var(--track-fill) 100.00%, var(--track) 100.00%");
  });
});

describe("formatSliderValue", () => {
  it("signs positive values of bipolar ranges only", () => {
    expect(formatSliderValue(0.5, -5, 0.01)).toBe("+0.50");
    expect(formatSliderValue(-12, -100, 1)).toBe("-12");
    expect(formatSliderValue(0, -100, 1)).toBe("0");
    expect(formatSliderValue(92, 50, 1)).toBe("92");
    expect(formatSliderValue(0.5, -5, 0.01, "EV")).toBe("+0.50 EV");
  });

  it("shows as many decimals as the step, and attaches degrees", () => {
    expect(formatSliderValue(-1.4, -15, 0.1, "°")).toBe("-1.4°");
    expect(formatSliderValue(0.5, -5, 0.01, "EV")).toBe("+0.50 EV");
    expect(formatSliderValue(40, 0, 1)).toBe("40");
  });
});
