import { describe, expect, it } from "vitest";
import { formatBytes, formatDateTime } from "./format";

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
