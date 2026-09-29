import { describe, expect, it } from "vitest";
import { fitSize, nextBoxShape } from "./viewerLayout";

describe("fitSize", () => {
  const space = { width: 1200, height: 800 };

  it("gives a small preview and a large render of the same photo the same size", () => {
    expect(fitSize(space, { width: 1500, height: 1000 })).toEqual(fitSize(space, { width: 3000, height: 2000 }));
    // Real frames round their sizes slightly differently: at most a pixel apart.
    const preview = fitSize(space, { width: 1024, height: 683 }); // embedded camera preview
    const detail = fitSize(space, { width: 3032, height: 2021 }); // full render
    expect(Math.abs(preview.width - detail.width)).toBeLessThanOrEqual(1);
    expect(Math.abs(preview.height - detail.height)).toBeLessThanOrEqual(1);
  });

  it("fits landscape by width and portrait by height", () => {
    expect(fitSize(space, { width: 3000, height: 1000 })).toEqual({ width: 1200, height: 400 });
    expect(fitSize(space, { width: 2000, height: 3000 })).toEqual({ width: 533, height: 800 });
  });

  it("is empty before layout", () => {
    expect(fitSize({ width: 0, height: 800 }, { width: 10, height: 10 })).toEqual({ width: 0, height: 0 });
  });
});

describe("nextBoxShape", () => {
  const full = { width: 6048, height: 4024 };

  it("keeps one box per photo, from its full size, for every render", () => {
    let s = nextBoxShape(null, 7, { width: 1516, height: 1010 }, full);
    expect(s).toEqual({ owner: 7, width: 6048, height: 4024 });
    s = nextBoxShape(s, 7, { width: 3024, height: 2011 }, full);
    expect(s).toEqual({ owner: 7, width: 6048, height: 4024 });
  });

  it("starts a new box for the next photo", () => {
    const s = nextBoxShape({ owner: 7, width: 6048, height: 4024 }, 8, { width: 1000, height: 1500 }, { width: 4000, height: 6000 });
    expect(s).toEqual({ owner: 8, width: 4000, height: 6000 });
  });

  it("falls back to the frame's shape if the full size is unknown", () => {
    expect(nextBoxShape(null, 9, { width: 1500, height: 1000 }, null)).toEqual({ owner: 9, width: 1500, height: 1000 });
  });
});
