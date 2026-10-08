import { describe, expect, it } from "vitest";
import { formatEstimate } from "./estimate";

describe("formatEstimate", () => {
  it("writes sizes as the design does", () => {
    expect(formatEstimate(4_230_000, 1)).toBe("≈ 4.2 MB");
    expect(formatEstimate(48_600_000, 1)).toBe("≈ 49 MB");
    expect(formatEstimate(850_400, 1)).toBe("≈ 850 KB");
    expect(formatEstimate(200, 1)).toBe("≈ 1 KB");
  });

  it("says each for several photos", () => {
    expect(formatEstimate(4_230_000, 12)).toBe("≈ 4.2 MB each");
  });
});
