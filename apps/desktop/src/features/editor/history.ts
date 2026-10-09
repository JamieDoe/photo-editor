import type { EditRecipe } from "../../ipc/generated/EditRecipe";

/**
 * Undo and redo of a photo's edit (ADR 0044). History is edit state, never pixels
 * (docs/PRODUCT.md §23): each step keeps the recipe from before the change, and
 * recipes share everything a change left alone (masks, curves, strokes), so a step
 * costs little more than what it changed.
 *
 * One step is one action:
 * - everything changed while a pointer is held down (a slider drag, moving a mask
 *   handle, a brush stroke, dragging the tone curve) is one step;
 * - otherwise, changes to the same controls less than `COALESCE_MS` apart (a slider
 *   nudged with the arrow keys) are one step;
 * - anything else (a click, Reset, a pasted edit) is its own step.
 */

/** Keyboard or other changes to the same controls closer than this are one step. */
export const COALESCE_MS = 1000;
/** Steps kept per photo: the oldest are dropped beyond this. */
export const MAX_STEPS = 200;

type Key = keyof EditRecipe;

/** The recipe's top-level fields that differ between `a` and `b` (by value for
 *  numbers and strings, by identity for objects, which edits replace when changed). */
export function changedKeys(a: EditRecipe, b: EditRecipe): Key[] {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)] as Key[]);
  return [...keys].filter((k) => a[k] !== b[k]).sort();
}

interface Step {
  /** The recipe before the change (undo) or after it (redo). */
  recipe: EditRecipe;
  /** What changed. */
  keys: Key[];
  label: string;
  /** When it last changed, and the pointer gesture it happened in (0: none). */
  at: number;
  gesture: number;
  /** Takes no further changes: its gesture ended, or it was undone or redone. */
  sealed: boolean;
}

export class EditHistory {
  private readonly past: Step[] = [];
  private readonly future: Step[] = [];
  private gesture = 0;
  private gestureActive = false;

  constructor(private readonly describe: (keys: Key[]) => string) {}

  /** A pointer went down: changes until it goes up are one step. */
  beginGesture(): void {
    this.gesture++;
    this.gestureActive = true;
  }

  /** The pointer went up. */
  endGesture(): void {
    if (!this.gestureActive) return;
    this.gestureActive = false;
    const last = this.past.at(-1);
    if (last?.gesture === this.gesture) last.sealed = true;
  }

  /** The edit changed from `before` to `after` at time `now` (ms). A `label` names a
   *  single action (applying a preset): it is a step of its own, never joined. */
  record(before: EditRecipe, after: EditRecipe, now: number, label?: string): void {
    const keys = changedKeys(before, after);
    if (keys.length === 0) return;
    if (label !== undefined) {
      this.future.length = 0;
      this.past.push({ recipe: before, keys, label, at: now, gesture: 0, sealed: true });
      if (this.past.length > MAX_STEPS) this.past.shift();
      return;
    }
    const gesture = this.gestureActive ? this.gesture : 0;
    const last = this.past.at(-1);
    const joins =
      last !== undefined &&
      !last.sealed &&
      (gesture !== 0 ? last.gesture === gesture : last.gesture === 0 && sameKeys(last.keys, keys) && now - last.at < COALESCE_MS);
    this.future.length = 0;
    if (last && joins) {
      last.at = now;
      last.keys = union(last.keys, keys);
      last.label = this.describe(last.keys);
      // Changed back to where the step started: nothing to undo.
      if (changedKeys(last.recipe, after).length === 0) this.past.pop();
      return;
    }
    this.past.push({ recipe: before, keys, label: this.describe(keys), at: now, gesture, sealed: false });
    if (this.past.length > MAX_STEPS) this.past.shift();
  }

  /** The recipe before the last step, given the current one; null if there is none. */
  undo(current: EditRecipe): EditRecipe | null {
    const step = this.past.pop();
    if (!step) return null;
    this.future.push({ ...step, recipe: current, sealed: true });
    // What comes next is a new step, however soon.
    const top = this.past.at(-1);
    if (top) top.sealed = true;
    return step.recipe;
  }

  /** The recipe after the last undone step, given the current one; null if none. */
  redo(current: EditRecipe): EditRecipe | null {
    const step = this.future.pop();
    if (!step) return null;
    this.past.push({ ...step, recipe: current, sealed: true });
    return step.recipe;
  }

  /** What Undo and Redo would change, or null when there is nothing to. */
  get undoLabel(): string | null {
    return this.past.at(-1)?.label ?? null;
  }

  get redoLabel(): string | null {
    return this.future.at(-1)?.label ?? null;
  }
}

const sameKeys = (a: readonly Key[], b: readonly Key[]) => a.length === b.length && a.every((k, i) => k === b[i]);
const union = (a: readonly Key[], b: readonly Key[]) => [...new Set([...a, ...b])].sort();

/** How the steps' fields are named, for "Undo Exposure". */
const GROUPS: Partial<Record<Key, string>> = {
  mixer: "Colour mixer",
  geometry: "Crop and geometry",
  chromaticAberration: "Lens corrections",
  profileCorrections: "Lens corrections",
  pointCurve: "Tone curve",
  channelCurves: "Tone curve",
  masks: "Masks",
  look: "Look",
};

/** A step's name: the control it changed ("Exposure"), its group ("Masks"), or
 *  "Edits" when it changed several. `labels` names the sliders (from their specs). */
export function describeChange(keys: readonly Key[], labels: ReadonlyMap<string, string>): string {
  const names = [...new Set(keys.map((k) => labels.get(k) ?? GROUPS[k] ?? k))];
  return names.length === 1 ? names[0]! : "Edits";
}
