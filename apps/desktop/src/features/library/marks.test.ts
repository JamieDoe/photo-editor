import { describe, expect, it } from "vitest";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { applyChange, flagClick, markChangeForKey, starClick, starsText, stepFrom, visiblePhotos } from "./marks";

describe("keyboard shortcuts", () => {
  it("maps digits to ratings and letters to flags", () => {
    expect(markChangeForKey("0")).toEqual({ type: "rating", stars: 0 });
    expect(markChangeForKey("5")).toEqual({ type: "rating", stars: 5 });
    expect(markChangeForKey("P")).toEqual({ type: "flag", flag: "pick" });
    expect(markChangeForKey("x")).toEqual({ type: "flag", flag: "reject" });
    expect(markChangeForKey("u")).toEqual({ type: "flag", flag: "none" });
    expect(markChangeForKey("6")).toBeNull();
    expect(markChangeForKey("Enter")).toBeNull();
  });
});

describe("clicks", () => {
  it("toggles the current star off and the active flag off", () => {
    expect(starClick(3, 3)).toEqual({ type: "rating", stars: 0 });
    expect(starClick(3, 4)).toEqual({ type: "rating", stars: 4 });
    expect(flagClick("pick", "pick")).toEqual({ type: "flag", flag: "none" });
    expect(flagClick("pick", "reject")).toEqual({ type: "flag", flag: "reject" });
  });

  it("applies one change without touching the other mark", () => {
    const m = { rating: 4, flag: "pick" as const };
    expect(applyChange(m, { type: "rating", stars: 1 })).toEqual({ rating: 1, flag: "pick" });
    expect(applyChange(m, { type: "flag", flag: "none" })).toEqual({ rating: 4, flag: "none" });
  });
});

const photo = (name: string, rating: number, flag: "none" | "pick" | "reject"): PhotoEntryDto => ({
  name,
  path: `/p/${name}`,
  sizeBytes: 1,
  modifiedMs: 0,
  raw: true,
  details: null,
  marks: { rating, flag },
  edited: false,
});

describe("visiblePhotos", () => {
  const photos = [photo("a", 0, "none"), photo("b", 3, "pick"), photo("c", 5, "reject"), photo("d", 2, "pick")];
  const names = (ps: PhotoEntryDto[]) => ps.map((p) => p.name);

  it("filters a folder by picks or three stars and up", () => {
    expect(names(visiblePhotos(photos, "all", null))).toEqual(["a", "b", "c", "d"]);
    expect(names(visiblePhotos(photos, "picks", null))).toEqual(["b", "d"]);
    expect(names(visiblePhotos(photos, "rated3", null))).toEqual(["b", "c"]);
  });

  it("drops photos that left the collection being viewed", () => {
    expect(names(visiblePhotos(photos, "all", "rejected"))).toEqual(["c"]);
    expect(names(visiblePhotos(photos, "rated3", "picks"))).toEqual(["b"]);
  });

  it("writes stars", () => {
    expect(starsText(3)).toBe("★★★");
    expect(starsText(0)).toBe("");
  });
});

describe("stepFrom", () => {
  const all = [photo("a", 0, "reject"), photo("b", 0, "none"), photo("c", 0, "reject"), photo("d", 0, "reject")];
  const rejected = visiblePhotos(all, "all", "rejected"); // a, c, d

  it("moves through the visible photos", () => {
    expect(stepFrom(all, rejected, "/p/c", 1)?.name).toBe("d");
    expect(stepFrom(all, rejected, "/p/c", -1)?.name).toBe("a");
    expect(stepFrom(all, rejected, "/p/d", 1)).toBeNull();
  });

  it("continues from where a just-filtered-out photo was", () => {
    // "b" is no longer rejected (not visible): → goes to c, ← to a.
    expect(stepFrom(all, rejected, "/p/b", 1)?.name).toBe("c");
    expect(stepFrom(all, rejected, "/p/b", -1)?.name).toBe("a");
    expect(stepFrom(all, rejected, "/p/zzz", 1)).toBeNull();
  });
});
