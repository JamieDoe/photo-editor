import type { CropRect } from "../../ipc/generated/CropRect";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Combine } from "../../ipc/generated/Combine";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { Mask } from "../../ipc/generated/Mask";
import type { MaskShape } from "../../ipc/generated/MaskShape";

/**
 * Masks in the editor (ADR 0040). Shapes are stored in the frame the crop is in
 * (fractions of the turned, straightened photo before cropping); the viewer shows the
 * crop, so points move between the two with `toShown` / `fromShown`.
 */

export type MaskKind = MaskShape["kind"];
export type Point = [number, number];

/** How the design names each kind, and its colour dot. */
export const MASK_KINDS: Record<MaskKind, { label: string; dot: string; add: string; hint: string }> = {
  linear: { label: "Linear gradient", dot: "var(--mask-linear)", add: "Linear", hint: "Graduated filter" },
  radial: { label: "Radial gradient", dot: "var(--mask-radial)", add: "Radial", hint: "Radial filter" },
  brush: { label: "Brush", dot: "var(--mask-brush)", add: "Brush", hint: "Paint an area" },
};

/** The kinds that can be added, in the design's order. */
export const ADDABLE_KINDS: readonly MaskKind[] = ["brush", "linear", "radial"];

/** The brush the next stroke is painted with (ADR 0042): its radius as a fraction of
 *  the frame's diagonal, its soft edge and flow (0..100), and whether it erases. */
export interface BrushSettings {
  size: number;
  feather: number;
  flow: number;
  erase: boolean;
}

export const DEFAULT_BRUSH: BrushSettings = { size: 0.04, feather: 50, flow: 100, erase: false };

/** The Size slider (1..100) and the brush radius it sets: 100 is a quarter of the
 *  frame's diagonal. */
export const brushSizeFromSlider = (v: number) => v / 400;
export const sliderFromBrushSize = (size: number) => Math.round(size * 400);

export const FULL_CROP: CropRect = { x: 0, y: 0, w: 1, h: 1 };

export function masksOf(r: EditRecipe): Mask[] {
  return r.masks ?? [];
}

/** `r` with `masks` (the field left out when empty, as the renderer writes it). */
export function withMasks(r: EditRecipe, masks: Mask[]): EditRecipe {
  return { ...r, masks: masks.length > 0 ? masks : undefined };
}

/** The mask's name in lists: its kind, numbered when there are several of it. */
export function maskName(masks: readonly Mask[], mask: Mask): string {
  const same = masks.filter((m) => m.shape.kind === mask.shape.kind);
  const label = MASK_KINDS[mask.shape.kind].label;
  return same.length > 1 ? `${label} ${same.indexOf(mask) + 1}` : label;
}

/** A frame point as a fraction of the shown (cropped) picture. */
export function toShown(p: Point, crop: CropRect): Point {
  return [(p[0] - crop.x) / crop.w, (p[1] - crop.y) / crop.h];
}

/** A point of the shown picture back in the frame. */
export function fromShown(p: Point, crop: CropRect): Point {
  return [crop.x + p[0] * crop.w, crop.y + p[1] * crop.h];
}

const nextId = (masks: readonly Mask[]) => masks.reduce((max, m) => Math.max(max, m.id), 0) + 1;
const NO_ADJUSTMENTS: LocalAdjustments = { exposure: 0, warmth: 0, clarity: 0 };

/** A new shape of `kind` over the shown picture:
 *  - linear: strongest at the top, fading out by a little past the middle (a sky, the
 *    common first use);
 *  - radial: a circle in the middle, a little under half the short side across,
 *    fading over half its radius;
 *  - brush: nothing painted yet. */
export function newShape(kind: MaskKind, crop: CropRect): MaskShape {
  switch (kind) {
    case "linear":
      return { kind: "linear", start: fromShown([0.5, 0.1], crop), end: fromShown([0.5, 0.55], crop) };
    case "radial": {
      const r = 0.15 * Math.min(crop.w, crop.h);
      return { kind: "radial", centre: fromShown([0.5, 0.5], crop), radius: [r, r], angle: 0, feather: 50 };
    }
    case "brush":
      return { kind: "brush", strokes: [] };
  }
}

export function newMask(kind: MaskKind, masks: readonly Mask[], crop: CropRect): Mask {
  return { id: nextId(masks), shape: newShape(kind, crop), adjustments: { ...NO_ADJUSTMENTS } };
}

/** How each way of combining a shape is named, and what it does. */
export const COMBINE_MODES: Record<Combine, { label: string; hint: string }> = {
  add: { label: "Add", hint: "Also adjust where this shape covers" },
  subtract: { label: "Subtract", hint: "Leave out where this shape covers" },
  intersect: { label: "Intersect", hint: "Adjust only where this shape and the ones before it overlap" },
};

/** A mask's shapes in order (ADR 0043): the first, then each further shape with how
 *  it combines. */
export function shapesOf(m: Mask): Array<{ shape: MaskShape; mode: Combine | null }> {
  return [{ shape: m.shape, mode: null }, ...(m.parts ?? []).map((p) => ({ shape: p.shape, mode: p.mode }))];
}

/** `m` with shape `i` (0 is the first) replaced. */
export function withShapeAt(m: Mask, i: number, shape: MaskShape): Mask {
  if (i === 0) return { ...m, shape };
  return { ...m, parts: (m.parts ?? []).map((p, k) => (k === i - 1 ? { ...p, shape } : p)) };
}

/** `m` with another shape, combined as `mode`. */
export function addShape(m: Mask, mode: Combine, shape: MaskShape): Mask {
  return { ...m, parts: [...(m.parts ?? []), { mode, shape }] };
}

/** `m` without shape `i`; removing the first makes the next the first. The last shape
 *  cannot be removed (remove the mask instead). */
export function removeShape(m: Mask, i: number): Mask {
  const parts = m.parts ?? [];
  if (parts.length === 0) return m;
  const rest = i === 0 ? { shape: parts[0]!.shape, parts: parts.slice(1) } : { shape: m.shape, parts: parts.filter((_, k) => k !== i - 1) };
  return { ...m, shape: rest.shape, parts: rest.parts.length > 0 ? rest.parts : undefined };
}

/** `m` with shape `i` (not the first) combined as `mode`. */
export function setShapeMode(m: Mask, i: number, mode: Combine): Mask {
  return { ...m, parts: (m.parts ?? []).map((p, k) => (k === i - 1 ? { ...p, mode } : p)) };
}

export function updateMask(masks: readonly Mask[], id: number, change: (m: Mask) => Mask): Mask[] {
  return masks.map((m) => (m.id === id ? change(m) : m));
}

export function setAdjustment(masks: readonly Mask[], id: number, key: keyof LocalAdjustments, value: number): Mask[] {
  return updateMask(masks, id, (m) => ({ ...m, adjustments: { ...m.adjustments, [key]: value } }));
}

export function adjusted(m: Mask): boolean {
  const a = m.adjustments;
  return a.exposure !== 0 || a.warmth !== 0 || a.clarity !== 0;
}

/** A shape with every point moved by `f` and a radial's angle changed by `turn`. */
function mapShape(shape: MaskShape, f: (p: Point) => Point, turn: (angle: number) => number): MaskShape {
  switch (shape.kind) {
    case "linear":
      return { ...shape, start: f(shape.start), end: f(shape.end) };
    case "radial":
      return { ...shape, centre: f(shape.centre), angle: normaliseAngle(turn(shape.angle)) };
    case "brush":
      // Sizes are fractions of the diagonal, which turns leave alone.
      return { ...shape, strokes: shape.strokes.map((s) => ({ ...s, points: s.points.map(f) })) };
  }
}

/** An angle in (-180, 180]. */
export function normaliseAngle(a: number): number {
  const r = ((a % 360) + 360) % 360;
  return r > 180 ? r - 360 : r;
}

const tidy = (v: number) => Math.round(v * 1e6) / 1e6;

/** `m` with every shape mapped as `mapShape` does. */
function mapMask(m: Mask, f: (p: Point) => Point, turn: (angle: number) => number): Mask {
  const parts = m.parts?.map((p) => ({ ...p, shape: mapShape(p.shape, f, turn) }));
  return { ...m, shape: mapShape(m.shape, f, turn), ...(parts ? { parts } : {}) };
}

/** Masks turned with the picture a quarter clockwise (1) or anticlockwise (-1), as
 *  the crop is (ADR 0039). */
export function turnMasks(masks: readonly Mask[], turn: 1 | -1): Mask[] {
  const f = ([x, y]: Point): Point => (turn === 1 ? [tidy(1 - y), x] : [y, tidy(1 - x)]);
  // Radii are fractions of the diagonal, which a turn leaves alone.
  return masks.map((m) => mapMask(m, f, (a) => a + 90 * turn));
}

/** Masks mirrored left to right with the picture. */
export function flipMasks(masks: readonly Mask[]): Mask[] {
  return masks.map((m) => mapMask(m, ([x, y]): Point => [tidy(1 - x), y], (a) => -a));
}

/**
 * How much a linear gradient covers shown point `p`, as the renderer computes it:
 * measured in pixels of a `w` x `h` picture (so the lines are perpendicular on screen),
 * `1 - smoothstep` from the start line to the end line.
 */
export function linearCoverage(start: Point, end: Point, p: Point, w: number, h: number): number {
  const d = [(end[0] - start[0]) * w, (end[1] - start[1]) * h];
  const len2 = Math.max(d[0]! * d[0]! + d[1]! * d[1]!, 1e-6);
  const t = Math.min(1, Math.max(0, (((p[0] - start[0]) * w) * d[0]! + ((p[1] - start[1]) * h) * d[1]!) / len2));
  return 1 - t * t * (3 - 2 * t);
}

/**
 * How much a radial gradient covers shown point `p`, as the renderer computes it, for a
 * frame of `w` x `h` pixels (radii are fractions of its diagonal).
 */
export function radialCoverage(
  shape: Extract<MaskShape, { kind: "radial" }>,
  p: Point,
  w: number,
  h: number,
): number {
  const diagonal = Math.hypot(w, h);
  const [a, b] = [shape.radius[0] * diagonal, shape.radius[1] * diagonal];
  const t = (shape.angle * Math.PI) / 180;
  const [dx, dy] = [(p[0] - shape.centre[0]) * w, (p[1] - shape.centre[1]) * h];
  const u = (dx * Math.cos(t) + dy * Math.sin(t)) / a;
  const v = (-dx * Math.sin(t) + dy * Math.cos(t)) / b;
  const d = Math.hypot(u, v);
  const inner = 1 - shape.feather / 100;
  if (inner >= 1) return d <= 1 ? 1 : 0;
  const s = Math.min(1, Math.max(0, (d - inner) / (1 - inner)));
  return 1 - s * s * (3 - 2 * s);
}
