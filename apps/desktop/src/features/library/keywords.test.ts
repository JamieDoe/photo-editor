import { describe, expect, it } from "vitest";
import { completeFragment, keywordSuggestions, splitKeywords, typedFragment } from "./keywords";

const use = (name: string, photos: number) => ({ name, photos });

describe("keywords as typed", () => {
  it("splits on commas, semicolons and lines, tidied and once each", () => {
    expect(splitKeywords(" heron,  grey   heron ;Heron\nLake,, ")).toEqual(["heron", "grey heron", "Lake"]);
    expect(splitKeywords("   ")).toEqual([]);
    expect(splitKeywords(`ok, ${"x".repeat(101)}`)).toEqual(["ok"]);
  });

  it("completes the keyword being typed", () => {
    expect(typedFragment("heron, gr")).toBe("gr");
    expect(typedFragment("heron, ")).toBe("");
    expect(completeFragment("gr", "grey heron")).toBe("grey heron");
    expect(completeFragment("heron, gr", "grey heron")).toBe("heron, grey heron");
  });

  it("suggests the library's keywords, those starting with it first", () => {
    const library = [use("Heron", 40), use("Grey heron", 3), use("Herring gull", 9), use("Lake", 12)];
    expect(keywordSuggestions(library, "lake, her", [], 1)).toEqual(["Heron", "Herring gull", "Grey heron"]);
    // Not one already on every photo; one on some of them still is.
    expect(keywordSuggestions(library, "her", [use("Heron", 2), use("Herring gull", 1)], 2)).toEqual([
      "Herring gull",
      "Grey heron",
    ]);
    // Nothing typed, or exactly a keyword: nothing to suggest for it.
    expect(keywordSuggestions(library, "heron, ", [], 1)).toEqual([]);
    expect(keywordSuggestions(library, "lake", [], 1)).toEqual([]);
    expect(keywordSuggestions(library, "e", [], 1, 2)).toEqual(["Heron", "Lake"]);
  });
});
