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
  totalMs: 120,
  ...over,
});

describe("indexStatusText", () => {
  it("shows progress while indexing", () => {
    expect(indexStatusText({ type: "progress", root: "/p", found: 600, processed: 0 }, null)).toBe(
      "Indexing… found 600 photos",
    );
    expect(indexStatusText({ type: "progress", root: "/p", found: 600, processed: 256 }, null)).toBe(
      "Indexing 256 of 600…",
    );
  });

  it("summarises only what changed", () => {
    expect(indexStatusText(null, finished())).toBe("600 photos indexed");
    expect(indexStatusText(null, finished({ new: 2, missing: 1 }))).toBe("600 photos indexed · 2 new · 1 missing");
  });

  it("says nothing before the first index", () => {
    expect(indexStatusText(null, null)).toBeNull();
  });
});
