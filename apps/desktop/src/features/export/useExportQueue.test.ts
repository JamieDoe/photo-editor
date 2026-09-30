import { describe, expect, it } from "vitest";
import { finishedMessage } from "./useExportQueue";

const run = (exported: number, failed = 0, cancelled = false) => ({
  type: "finished" as const,
  exported,
  outputs: [],
  failed: Array.from({ length: failed }, (_, i) => ({ file: `f${i}.nef`, message: "no" })),
  folder: "/Users/me/Pictures/Lake District",
  cancelled,
});

describe("export queue messages", () => {
  it("names the count and the folder", () => {
    expect(finishedMessage(run(12))).toBe("Exported 12 photos to Lake District");
    expect(finishedMessage(run(1))).toBe("Exported 1 photo to Lake District");
    expect(finishedMessage(run(3, 1))).toBe("Exported 3 photos to Lake District · 1 photo couldn’t be exported");
    expect(finishedMessage(run(0, 2))).toBe("Nothing exported · 2 photos couldn’t be exported");
  });

  it("says when it was stopped", () => {
    expect(finishedMessage(run(0, 0, true))).toBe("Export cancelled");
    expect(finishedMessage(run(4, 0, true))).toBe("Export stopped after 4 photos");
  });
});
