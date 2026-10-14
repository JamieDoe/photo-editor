import { describe, expect, it } from "vitest";
import { filterNotices, type NoticeGroup } from "./notices";

const entry = (name: string, licence: string) => ({ name, version: "1.0.0", licence, source: "", texts: [] });
const groups: NoticeGroup[] = [
  { title: "Native libraries", entries: [entry("LibRaw", "LGPL-2.1 OR CDDL-1.0")] },
  { title: "Rust crates", entries: [entry("serde", "MIT OR Apache-2.0"), entry("libc", "MIT OR Apache-2.0")] },
];

describe("filterNotices", () => {
  it("keeps everything without a query", () => {
    expect(filterNotices(groups, "  ")).toBe(groups);
  });

  it("matches names and licences, ignoring case, and drops empty groups", () => {
    expect(filterNotices(groups, "LIB").flatMap((g) => g.entries.map((e) => e.name))).toEqual(["LibRaw", "libc"]);
    expect(filterNotices(groups, "lgpl").map((g) => g.title)).toEqual(["Native libraries"]);
    expect(filterNotices(groups, "gpl-3")).toEqual([]);
  });
});
