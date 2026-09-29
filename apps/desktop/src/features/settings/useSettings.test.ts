import { describe, expect, it } from "vitest";
import { applyTheme } from "./useSettings";

function fakeRoot() {
  const attrs = new Map<string, string>();
  return {
    attrs,
    setAttribute: (k: string, v: string) => attrs.set(k, v),
    removeAttribute: (k: string) => attrs.delete(k),
  } as unknown as HTMLElement & { attrs: Map<string, string> };
}

describe("applyTheme", () => {
  it("sets an explicit theme and clears it for system", () => {
    const root = fakeRoot();
    applyTheme("light", root);
    expect(root.attrs.get("data-theme")).toBe("light");
    applyTheme("system", root);
    expect(root.attrs.has("data-theme")).toBe(false);
  });
});
