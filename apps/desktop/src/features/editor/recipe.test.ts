import { describe, expect, it } from "vitest";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import { defaultRecipe, isIdentity } from "./recipe";

const specs: AdjustmentSpec[] = [
  { key: "exposure", label: "Exposure", group: "Light", min: -5, max: 5, step: 0.01, default: 0, more: false, unit: "EV" },
];

describe("recipe helpers", () => {
  it("defaults to the Standard look with neutral adjustments", () => {
    const r = defaultRecipe(2, specs);
    expect(r.look).toBe("standard");
    expect(isIdentity(r)).toBe(true);
  });

  it("counts the Flat look, or any adjustment, as an edit", () => {
    const r = defaultRecipe(2, specs);
    expect(isIdentity({ ...r, look: "flat" })).toBe(false);
    expect(isIdentity({ ...r, exposure: 0.1 })).toBe(false);
  });
});
