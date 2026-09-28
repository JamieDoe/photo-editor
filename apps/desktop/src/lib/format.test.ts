import { describe, expect, it } from "vitest";
import { formatAperture, formatBytes, formatCaptured, formatDateTime, formatExposure, formatShutter } from "./format";

describe("formatBytes", () => {
  it("uses decimal units with sensible precision", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1_500)).toBe("1.5 KB");
    expect(formatBytes(25_639_680)).toBe("26 MB");
    expect(formatBytes(3_200_000_000)).toBe("3.2 GB");
  });
});

describe("formatDateTime", () => {
  it("shows a dash for unknown times", () => {
    expect(formatDateTime(0)).toBe("—");
  });
  it("formats known times", () => {
    expect(formatDateTime(Date.UTC(2026, 8, 28, 10, 0), "en-GB")).toMatch(/2026/);
  });
});

describe("exposure formatting", () => {
  it("writes shutter speeds the way photographers do", () => {
    expect(formatShutter(1 / 250)).toBe("1/250 s");
    expect(formatShutter(0.0025)).toBe("1/400 s");
    expect(formatShutter(0.4)).toBe("0.4 s");
    expect(formatShutter(4)).toBe("4 s");
    expect(formatShutter(0)).toBeNull();
  });

  it("formats apertures and the combined line", () => {
    expect(formatAperture(8)).toBe("f/8");
    expect(formatAperture(6.7)).toBe("f/6.7");
    expect(formatExposure({ iso: 100, focalLengthMm: 52, aperture: 6.7, shutterSeconds: 1 })).toBe("ISO 100 · 52 mm · f/6.7 · 1 s");
    expect(formatExposure({ aperture: 2 })).toBe("f/2");
    expect(formatExposure({})).toBe("");
  });

  it("shows capture times exactly as recorded, whatever the local zone", () => {
    const text = formatCaptured("2026-09-24T06:41:12", "en-GB");
    expect(text).toMatch(/24 Sept? 2026/);
    expect(text).toMatch(/06:41/);
    expect(formatCaptured(null)).toBeNull();
    expect(formatCaptured("garbage")).toBeNull();
  });
});
