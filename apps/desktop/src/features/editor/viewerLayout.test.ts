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
  const preview = { source: "embedded" as const, frame: { width: 1616, height: 1080 } };
  const render = (imageId: number, width = 1516, height = 1010) => ({ source: "render" as const, imageId, frame: { width, height } });
  const full = { width: 6048, height: 4024 };

  it("keeps the camera preview's box for every render of that opening", () => {
    let s = nextBoxShape(null, preview, null);
    expect(s).toEqual({ owner: "opening", width: 1616, height: 1080 });
    s = nextBoxShape(s, render(7), full);
    expect(s).toEqual({ owner: 7, width: 1616, height: 1080 });
    s = nextBoxShape(s, render(7, 3024, 2012), full);
    expect(s).toEqual({ owner: 7, width: 1616, height: 1080 });
  });

  it("starts a new box when the next photo opens", () => {
    let s = nextBoxShape(null, preview, null);
    s = nextBoxShape(s, render(7), full);
    s = nextBoxShape(s, { source: "embedded", frame: { width: 1080, height: 1616 } }, null); // portrait
    expect(s).toEqual({ owner: "opening", width: 1080, height: 1616 });
    expect(nextBoxShape(s, render(8), null).owner).toBe(8);
  });

  it("uses the full size for photos without a camera preview", () => {
    const s = nextBoxShape({ owner: 7, width: 3, height: 2 }, render(9, 1499, 1000), { width: 4000, height: 2667 });
    expect(s).toEqual({ owner: 9, width: 4000, height: 2667 });
  });
});
