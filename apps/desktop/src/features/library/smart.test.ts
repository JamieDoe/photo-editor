import { describe, expect, it } from "vitest";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { NO_RULES, hasNoRules, matchesMarks, rulesSummary } from "./smart";

const photo = (rating: number, flag: "none" | "pick" | "reject", label: PhotoEntryDto["marks"]["label"], edited: boolean): PhotoEntryDto => ({
  name: "a.nef",
  path: "/p/a.nef",
  sizeBytes: 1,
  modifiedMs: 0,
  raw: true,
  details: null,
  marks: { rating, flag, label },
  edited,
});

describe("smart collections", () => {
  it("match marks as the catalogue does", () => {
    const best = photo(4, "pick", "red", true);
    const plain = photo(0, "none", "none", false);
    const rejected = photo(2, "reject", "none", false);
    expect(matchesMarks(best, NO_RULES)).toBe(true);
    expect(matchesMarks(best, { ...NO_RULES, rating: 3 })).toBe(true);
    expect(matchesMarks(best, { ...NO_RULES, rating: 5 })).toBe(false);
    expect(matchesMarks(plain, { ...NO_RULES, rating: 0 })).toBe(true);
    expect(matchesMarks(best, { ...NO_RULES, rating: 0 })).toBe(false);
    expect(matchesMarks(best, { ...NO_RULES, flag: "picked" })).toBe(true);
    expect(matchesMarks(plain, { ...NO_RULES, flag: "unflagged" })).toBe(true);
    expect([best, plain, rejected].map((p) => matchesMarks(p, { ...NO_RULES, flag: "notRejected" }))).toEqual([true, true, false]);
    expect(matchesMarks(rejected, { ...NO_RULES, flag: "rejected" })).toBe(true);
    expect(matchesMarks(best, { ...NO_RULES, label: "red" })).toBe(true);
    expect(matchesMarks(best, { ...NO_RULES, label: "none" })).toBe(false);
    expect(matchesMarks(plain, { ...NO_RULES, label: "none" })).toBe(true);
    expect(matchesMarks(best, { ...NO_RULES, edited: false })).toBe(false);
    // Words are left to the catalogue.
    expect(matchesMarks(plain, { ...NO_RULES, words: "iceland" })).toBe(true);
    // Every rule must match.
    expect(matchesMarks(best, { ...NO_RULES, rating: 3, flag: "rejected" })).toBe(false);
  });

  it("need at least one rule", () => {
    expect(hasNoRules(NO_RULES)).toBe(true);
    expect(hasNoRules({ ...NO_RULES, words: "   " })).toBe(true);
    expect(hasNoRules({ ...NO_RULES, edited: false })).toBe(false);
    expect(hasNoRules({ ...NO_RULES, rating: 0 })).toBe(false);
    expect(hasNoRules({ ...NO_RULES, keyword: " " })).toBe(true);
    expect(hasNoRules({ ...NO_RULES, keyword: "Heron" })).toBe(false);
  });

  it("read as a short summary", () => {
    expect(
      rulesSummary({ rating: 3, flag: "picked", label: "red", edited: true, words: " iceland   2026 ", keyword: "Heron" }),
    ).toBe("★ 3 and up · Picked · Red · Edited · Keyword Heron · “iceland 2026”");
    expect(rulesSummary({ ...NO_RULES, label: "none", edited: false })).toBe("No label · Not edited");
    expect(rulesSummary(NO_RULES)).toBe("");
  });
});
