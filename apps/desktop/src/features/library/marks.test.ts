import { describe, expect, it } from "vitest";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import type { ColourLabelDto } from "../../ipc/generated/ColourLabelDto";
import {
  COLLECTION_NAMES,
  inCollection,
  keepsOwnOrder,
  applyChange,
  flagClick,
  labelClick,
  markChangeForKey,
  rangeToTick,
  sortPhotos,
  starClick,
  starsText,
  stepFrom,
  visiblePhotos,
} from "./marks";

describe("keyboard shortcuts", () => {
  it("maps digits to ratings and letters to flags", () => {
    expect(markChangeForKey("0")).toEqual({ type: "rating", stars: 0 });
    expect(markChangeForKey("5")).toEqual({ type: "rating", stars: 5 });
    expect(markChangeForKey("P")).toEqual({ type: "flag", flag: "pick" });
    expect(markChangeForKey("x")).toEqual({ type: "flag", flag: "reject" });
    expect(markChangeForKey("u")).toEqual({ type: "flag", flag: "none" });
    expect(markChangeForKey("Enter")).toBeNull();
  });

  it("maps 6–9 to red, yellow, green and blue, and removes the label a photo has", () => {
    expect(markChangeForKey("6")).toEqual({ type: "label", label: "red" });
    expect(markChangeForKey("9")).toEqual({ type: "label", label: "blue" });
    const red = { rating: 0, flag: "none" as const, label: "red" as const };
    expect(markChangeForKey("6", red)).toEqual({ type: "label", label: "none" });
    expect(markChangeForKey("7", red)).toEqual({ type: "label", label: "yellow" });
  });
});

describe("clicks", () => {
  it("toggles the current star off and the active flag off", () => {
    expect(starClick(3, 3)).toEqual({ type: "rating", stars: 0 });
    expect(starClick(3, 4)).toEqual({ type: "rating", stars: 4 });
    expect(flagClick("pick", "pick")).toEqual({ type: "flag", flag: "none" });
    expect(flagClick("pick", "reject")).toEqual({ type: "flag", flag: "reject" });
    expect(labelClick("green", "green")).toEqual({ type: "label", label: "none" });
    expect(labelClick("green", "purple")).toEqual({ type: "label", label: "purple" });
  });

  it("applies one change without touching the other marks", () => {
    const m = { rating: 4, flag: "pick" as const, label: "blue" as const };
    expect(applyChange(m, { type: "rating", stars: 1 })).toEqual({ rating: 1, flag: "pick", label: "blue" });
    expect(applyChange(m, { type: "flag", flag: "none" })).toEqual({ rating: 4, flag: "none", label: "blue" });
    expect(applyChange(m, { type: "label", label: "red" })).toEqual({ rating: 4, flag: "pick", label: "red" });
  });
});

const photo = (
  name: string,
  rating: number,
  flag: "none" | "pick" | "reject",
  label: ColourLabelDto = "none",
  capturedAt: string | null = null,
): PhotoEntryDto => ({
  name,
  path: `/p/${name}`,
  sizeBytes: 1,
  modifiedMs: 0,
  raw: true,
  details:
    capturedAt === null
      ? null
      : { camera: null, lens: null, capturedAt, iso: null, aperture: null, shutterSeconds: null, focalLengthMm: null, width: null, height: null },
  marks: { rating, flag, label },
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

  it("narrows to one colour label", () => {
    const labelled = [photo("a", 0, "none", "red"), photo("b", 3, "pick", "green"), photo("c", 5, "none", "red")];
    expect(names(visiblePhotos(labelled, "all", null, "red"))).toEqual(["a", "c"]);
    expect(names(visiblePhotos(labelled, "rated3", null, "red"))).toEqual(["c"]);
    expect(names(visiblePhotos(labelled, "all", null, "purple"))).toEqual([]);
    expect(names(visiblePhotos(labelled, "all", null, null))).toEqual(["a", "b", "c"]);
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

describe("sortPhotos", () => {
  const names = (ps: PhotoEntryDto[]) => ps.map((p) => p.name);
  const photos = [
    photo("DSC_10.NEF", 2, "none", "none", "2026-09-24T08:00:00"),
    photo("DSC_9.NEF", 5, "none", "none", "2026-09-25T07:00:00"),
    photo("dsc_2.nef", 0, "none", "none", null),
    photo("DSC_1.NEF", 5, "none", "none", "2026-09-24T06:00:00"),
    photo("IMG_3.JPG", 0, "none", "none", null),
  ];

  it("orders by capture time, either way, with photos not indexed yet last by name", () => {
    expect(names(sortPhotos(photos, "captured"))).toEqual(["DSC_1.NEF", "DSC_10.NEF", "DSC_9.NEF", "dsc_2.nef", "IMG_3.JPG"]);
    expect(names(sortPhotos(photos, "newest"))).toEqual(["DSC_9.NEF", "DSC_10.NEF", "DSC_1.NEF", "dsc_2.nef", "IMG_3.JPG"]);
  });

  it("orders names as people read them: 9 before 10, case ignored", () => {
    expect(names(sortPhotos(photos, "name"))).toEqual(["DSC_1.NEF", "dsc_2.nef", "DSC_9.NEF", "DSC_10.NEF", "IMG_3.JPG"]);
  });

  it("orders by rating, highest first, then capture time", () => {
    expect(names(sortPhotos(photos, "rating"))).toEqual(["DSC_1.NEF", "DSC_9.NEF", "DSC_10.NEF", "dsc_2.nef", "IMG_3.JPG"]);
  });

  it("leaves the input as it was", () => {
    const before = names(photos);
    sortPhotos(photos, "name");
    expect(names(photos)).toEqual(before);
  });

  // A folder of n photos with mixed capture times, names and ratings.
  const folder = (n: number) =>
    Array.from({ length: n }, (_, i) =>
      photo(`DSC_${(i * 7919) % n}.NEF`, i % 6, "none", "none", i % 10 === 0 ? null : `2026-09-${String(1 + (i % 28)).padStart(2, "0")}T${String(i % 24).padStart(2, "0")}:00:00`),
    );

  it("does n log n work, not quadratic", () => {
    // Counts reads of the photos rather than timing them, so a busy machine can't fail it.
    // Ten times the photos is ~15× the reads for n log n and ~100× for a quadratic sort.
    const reads = (n: number, sort: "captured" | "name" | "rating") => {
      let count = 0;
      const counted = folder(n).map((p) => new Proxy(p, { get: (t, k) => (count++, Reflect.get(t, k)) }));
      sortPhotos(counted, sort);
      return count;
    };
    for (const sort of ["captured", "name", "rating"] as const) {
      expect(reads(10_000, sort) / reads(1_000, sort)).toBeLessThan(25);
    }
  });

  it("sorts a large folder", () => {
    // The timings in PERFORMANCE.md §46. Logged, not asserted: wall-clock time is too noisy to gate on.
    const many = folder(20_000);
    for (const sort of ["captured", "name", "rating"] as const) {
      const t = performance.now();
      expect(sortPhotos(many, sort)).toHaveLength(20_000);
      console.log(`sort ${sort} 20k: ${(performance.now() - t).toFixed(1)} ms`);
    }
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

  it("ticks a range from the last photo ticked, either way round", () => {
    const shown = ["a", "b", "c", "d", "e"];
    expect(rangeToTick(shown, "b", "d")).toEqual(["b", "c", "d"]);
    expect(rangeToTick(shown, "d", "b")).toEqual(["b", "c", "d"]);
    expect(rangeToTick(shown, null, "c")).toEqual(["c"]);
    // The anchor filtered out of view: just the photo clicked.
    expect(rangeToTick(shown, "z", "c")).toEqual(["c"]);
    expect(rangeToTick(shown, "a", "z")).toEqual([]);
  });
});

describe("recently edited", () => {
  it("keeps the catalogue's order, the latest edit first, and is named for it", () => {
    expect(keepsOwnOrder("edited")).toBe(true);
    for (const kind of ["all", "recent", "picks", "rated", "rejected"] as const) expect(keepsOwnOrder(kind)).toBe(false);
    expect(keepsOwnOrder(null)).toBe(false);
    expect(COLLECTION_NAMES.edited).toBe("Recently edited");
  });
});

describe("favourites", () => {
  it("are the five-star photos, as their marks change", () => {
    const marks = (rating: number) => ({ rating, flag: "none" as const, label: "none" as const });
    expect(inCollection(marks(5), "favourites")).toBe(true);
    expect(inCollection(marks(4), "favourites")).toBe(false);
    expect(COLLECTION_NAMES.favourites).toBe("Favourites");
  });
});

