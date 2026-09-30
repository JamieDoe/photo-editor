import { describe, expect, it } from "vitest";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { COALESCE_MS, EditHistory, MAX_STEPS, changedKeys, describeChange } from "./history";
import { neutralRecipe } from "./recipe";

const labels = new Map([
  ["exposure", "Exposure"],
  ["contrast", "Contrast"],
]);
const history = () => new EditHistory((keys) => describeChange(keys, labels));
const base = neutralRecipe(20);
const set = (r: EditRecipe, change: Partial<EditRecipe>): EditRecipe => ({ ...r, ...change });

/** Applies changes to `h`, recording each; returns the final recipe. */
function apply(h: EditHistory, start: EditRecipe, changes: Array<[Partial<EditRecipe>, number]>): EditRecipe {
  return changes.reduce((r, [change, at]) => {
    const next = set(r, change);
    h.record(r, next, at);
    return next;
  }, start);
}

describe("edit history", () => {
  it("undoes and redoes steps in order", () => {
    const h = history();
    const a = apply(h, base, [[{ exposure: 0.5 }, 0]]);
    const b = apply(h, a, [[{ contrast: 10 }, 5000]]);
    expect(h.undoLabel).toBe("Contrast");
    expect(h.undo(b)).toBe(a);
    expect([h.undoLabel, h.redoLabel]).toEqual(["Exposure", "Contrast"]);
    expect(h.undo(a)).toBe(base);
    expect(h.undo(base)).toBeNull();
    expect(h.redo(base)).toBe(a);
    expect(h.redo(a)).toBe(b);
    expect(h.redo(b)).toBeNull();
  });

  it("makes a drag one step", () => {
    const h = history();
    h.beginGesture();
    const end = apply(h, base, [
      [{ exposure: 0.1 }, 0],
      [{ exposure: 0.4 }, 3000],
      [{ exposure: 0.7 }, 9000],
    ]);
    h.endGesture();
    expect(h.undo(end)).toBe(base);
    expect(h.undo(base)).toBeNull();
  });

  it("keeps a gesture that touched several controls together, and the next one apart", () => {
    const h = history();
    h.beginGesture();
    const mid = apply(h, base, [
      [{ exposure: 0.2 }, 0],
      [{ contrast: 5 }, 10],
    ]);
    h.endGesture();
    expect(h.undoLabel).toBe("Edits");
    h.beginGesture();
    const end = apply(h, mid, [[{ exposure: 0.4 }, 20]]);
    h.endGesture();
    expect(h.undo(end)).toBe(mid);
  });

  it("joins quick changes to the same control, not slow ones or other controls", () => {
    const h = history();
    const nudged = apply(h, base, [
      [{ exposure: 0.01 }, 0],
      [{ exposure: 0.02 }, 200],
      [{ exposure: 0.03 }, 400],
    ]);
    const later = apply(h, nudged, [[{ exposure: 0.04 }, 400 + COALESCE_MS]]);
    const other = apply(h, later, [[{ contrast: 3 }, 400 + COALESCE_MS + 10]]);
    expect(h.undo(other)).toBe(later);
    expect(h.undo(later)).toBe(nudged);
    expect(h.undo(nudged)).toBe(base);
  });

  it("drops a step that ends where it started", () => {
    const h = history();
    h.beginGesture();
    apply(h, base, [
      [{ exposure: 0.5 }, 0],
      [{ exposure: 0 }, 10],
    ]);
    h.endGesture();
    expect(h.undoLabel).toBeNull();
  });

  it("starts afresh after an undo, and a new change clears redo", () => {
    const h = history();
    const a = apply(h, base, [[{ exposure: 0.1 }, 0]]);
    const b = apply(h, a, [[{ contrast: 4 }, 2000]]);
    expect(h.undo(b)).toBe(a);
    // Straight after undoing, another exposure change is its own step.
    const c = apply(h, a, [[{ exposure: 0.2 }, 2010]]);
    expect(h.redoLabel).toBeNull();
    expect(h.undo(c)).toBe(a);
    expect(h.undo(a)).toBe(base);
  });

  it("ignores changes that change nothing, and keeps a bounded number of steps", () => {
    const h = history();
    h.record(base, { ...base }, 0);
    expect(h.undoLabel).toBeNull();
    let r = base;
    for (let i = 1; i <= MAX_STEPS + 5; i++) r = apply(h, r, [[{ exposure: i / 1000 }, i * 10 * COALESCE_MS]]);
    let undone = 0;
    for (let prev = h.undo(r); prev; prev = h.undo(r)) {
      r = prev;
      undone++;
    }
    // The oldest five steps were dropped.
    expect(undone).toBe(MAX_STEPS);
    expect(r.exposure).toBe(5 / 1000);
  });

  it("compares top-level fields", () => {
    const masks = [{ id: 1, shape: { kind: "brush" as const, strokes: [] }, adjustments: { exposure: 1, warmth: 0, clarity: 0 } }];
    expect(changedKeys(base, set(base, { exposure: 1, masks }))).toEqual(["exposure", "masks"]);
    expect(describeChange(["masks"], labels)).toBe("Masks");
    expect(describeChange(["pointCurve", "channelCurves"], labels)).toBe("Tone curve");
  });
});
