import { describe, expect, it } from "vitest";
import { indexStatusText } from "./indexStatus";

const finished = (over: Partial<Record<string, number>> = {}) => ({
  type: "finished" as const,
  root: "/p",
  found: 600,
  new: 0,
  changed: 0,
  moved: 0,
  missing: 0,
  skipped: 0,
  detailsRead: 0,
  totalMs: 120,
  ...over,
});

describe("indexStatusText", () => {
  it("shows progress while indexing", () => {
    const progress = (stage: "recording" | "readingDetails", processed: number) =>
      ({ type: "progress", root: "/p", stage, total: 600, processed }) as const;
    expect(indexStatusText(progress("recording", 0), null)).toBe("Indexing… found 600 photos");
    expect(indexStatusText(progress("recording", 256), null)).toBe("Indexing 256 of 600…");
    expect(indexStatusText(progress("readingDetails", 256), null)).toBe("Reading photo details 256 of 600…");
  });

  it("summarises only what changed", () => {
    expect(indexStatusText(null, finished())).toBe("600 photos indexed");
    expect(indexStatusText(null, finished({ new: 2, missing: 1 }))).toBe("600 photos indexed · 2 new · 1 missing");
  });

  it("says nothing before the first index", () => {
    expect(indexStatusText(null, null)).toBeNull();
  });
});
