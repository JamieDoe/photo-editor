import { describe, expect, it } from "vitest";
import { gridLayout, scrollToReveal, visibleRange } from "./virtual";

describe("visibleRange", () => {
  const base = { viewportHeight: 600, listTop: 0, rowHeight: 100, rowCount: 1000, overscan: 1 };

  it("covers the viewport plus overscan", () => {
    expect(visibleRange({ ...base, scrollTop: 0 })).toEqual({ first: 0, end: 7 });
    expect(visibleRange({ ...base, scrollTop: 1050 })).toEqual({ first: 9, end: 18 });
  });

  it("accounts for content above the list", () => {
    // 300 px of folder chips above the list: the first rows are still visible.
    expect(visibleRange({ ...base, listTop: 300, scrollTop: 250 })).toEqual({ first: 0, end: 7 });
    expect(visibleRange({ ...base, listTop: 300, scrollTop: 0 })).toEqual({ first: 0, end: 4 });
  });

  it("clamps at the end and handles empty lists", () => {
    expect(visibleRange({ ...base, rowCount: 5, scrollTop: 0 })).toEqual({ first: 0, end: 5 });
    expect(visibleRange({ ...base, scrollTop: 99_900 })).toEqual({ first: 998, end: 1000 });
    expect(visibleRange({ ...base, rowCount: 0, scrollTop: 0 })).toEqual({ first: 0, end: 0 });
  });
});

describe("gridLayout", () => {
  it("adds columns so cards stay at most 280 px wide", () => {
    const wide = gridLayout(1152);
    expect(wide.columns).toBe(4);
    expect(wide.cardWidth).toBeLessThanOrEqual(280);
    expect(gridLayout(1400).columns).toBe(5);
  });

  it("keeps at least two columns and a 3:2 card plus caption per row", () => {
    const narrow = gridLayout(300);
    expect(narrow.columns).toBe(2);
    expect(narrow.rowHeight).toBe(Math.round((narrow.cardWidth * 2) / 3 + 26 + 20));
  });
});

describe("scrollToReveal", () => {
  const v = { scrollTop: 1000, viewportHeight: 500, listTop: 100, rowHeight: 100 };
  it("scrolls up or down just enough, or not at all", () => {
    expect(scrollToReveal({ ...v, rowIndex: 10 })).toBeNull(); // 1100..1200 is visible
    expect(scrollToReveal({ ...v, rowIndex: 5 })).toBe(600); // above: align top
    expect(scrollToReveal({ ...v, rowIndex: 14 })).toBe(1100); // below: align bottom (1500..1600)
  });
});
