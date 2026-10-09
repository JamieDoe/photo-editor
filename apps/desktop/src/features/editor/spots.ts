import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { RedEye } from "../../ipc/generated/RedEye";
import type { Geometry } from "../../ipc/generated/Geometry";
import type { Removal } from "../../ipc/generated/Removal";
import type { Stroke } from "../../ipc/generated/Stroke";
import type { Spot } from "../../ipc/generated/Spot";
import type { SpotKind } from "../../ipc/generated/SpotKind";
import { orientedSize, viewToSource } from "./cropGeometry";
import type { Point } from "./masks";

/**
 * Heal and clone spots (ADR 0054). Spots are in the source photo's coordinates
 * (fractions of its width and height, the radius a fraction of its long edge), so they
 * stay on what they cover whatever the geometry; the photo is shown in the frame
 * (turned, straightened, before the crop) that masks are drawn in. These map between
 * the two.
 */

/** A retouch tool: Remove paints areas to fill (ADR 0066); Heal and Clone place spots;
 *  Red eye fixes pupils (ADR 0080). */
export type RetouchToolKind = "remove" | SpotKind | "redEye";

/** The retouch tools, as in the design. */
export const RETOUCH_TOOLS: ReadonlyArray<{ kind: RetouchToolKind; label: string; hint: string; text: string }> = [
  {
    kind: "remove",
    label: "Remove",
    hint: "Paint over anything to erase it",
    text: "Paint over anything distracting — people, litter, power lines — and it’s filled in from its surroundings.",
  },
  { kind: "heal", label: "Heal", hint: "Blend texture from a nearby area", text: "Blends texture and tone from a nearby area. Good for blemishes and small marks." },
  {
    kind: "clone",
    label: "Clone",
    hint: "Copy pixels exactly",
    text: "Copies pixels exactly from a nearby area. Drag the source circle, or Option-click, to choose where from.",
  },
  {
    kind: "redEye",
    label: "Red eye",
    hint: "Fix red pupils from a flash",
    text: "Click on an eye with red from a flash: the red pupil is found and made dark. Make the brush larger if it isn’t found.",
  },
];

/** Red-eye corrections (ADR 0080), in the source photo's coordinates like spots. */
export function redEyesOf(r: EditRecipe): RedEye[] {
  return r.redEyes ?? [];
}

/** `r` with `eyes`, left out when there are none (as the renderer writes it). */
export function withRedEyes(r: EditRecipe, eyes: RedEye[]): EditRecipe {
  const { redEyes: _, ...rest } = r;
  return eyes.length > 0 ? { ...rest, redEyes: eyes } : rest;
}

/** Index of the red-eye correction under source point `p` (the last made wins), or
 *  null. `aspect` is width / height. */
export function redEyeAt(eyes: readonly RedEye[], p: Point, aspect: number): number | null {
  const [sx, sy] = aspect >= 1 ? [1, 1 / aspect] : [aspect, 1];
  for (let i = eyes.length - 1; i >= 0; i--) {
    const e = eyes[i]!;
    if (Math.hypot((p[0] - e.x) * sx, (p[1] - e.y) * sy) <= e.radius) return i;
  }
  return null;
}

/** `eye` moved by `delta` (source fractions). */
export function moveRedEye(eye: RedEye, delta: Point): RedEye {
  const clamp = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 1e5) / 1e5;
  return { ...eye, x: clamp(eye.x + delta[0]), y: clamp(eye.y + delta[1]) };
}

/** The design's Brush size range and starting value: the spot's diameter on screen. */
export const BRUSH_SIZE = { min: 5, max: 200, initial: 40 } as const;

export function spotsOf(r: EditRecipe): Spot[] {
  return r.spots ?? [];
}

/** `r` with `spots`, left out when there are none (as the renderer writes it). */
export function withSpots(r: EditRecipe, spots: Spot[]): EditRecipe {
  const { spots: _, ...rest } = r;
  return spots.length > 0 ? { ...rest, spots } : rest;
}

/** The geometry's parts that move the photo in the frame (not the crop). */
type Placement = Pick<Geometry, "straighten" | "vertical" | "horizontal" | "rotation" | "flip">;

const NO_GEOMETRY: Placement = { straighten: 0, vertical: 0, horizontal: 0, rotation: 0, flip: false };

export function placementOf(g: Geometry | undefined | null): Placement {
  return g ?? NO_GEOMETRY;
}

/** Where frame point `p` (fractions) is in the source (fractions of a `w` x `h`
 *  photo). Mirrors the renderer's `Mapping::source`: straighten and perspective
 *  undone in the turned photo, then the quarter turns, then the flip. */
export function frameToSource(g: Placement, w: number, h: number): (p: Point) => Point {
  const { width: vw, height: vh } = orientedSize(g, w, h);
  const view = viewToSource(g, vw, vh);
  const turns = ((g.rotation % 4) + 4) % 4;
  return ([fx, fy]) => {
    let [x, y] = view(fx, fy);
    let [cw, ch] = [vw, vh];
    for (let i = 0; i < turns; i++) {
      [x, y] = [y, cw - x];
      [cw, ch] = [ch, cw];
    }
    if (g.flip) x = w - x;
    return [x / w, y / h];
  };
}

/** Where source point `p` (fractions) shows in the frame: `frameToSource` inverted by
 *  Newton's method (the mapping is smooth; a few steps from the middle reach it). */
export function sourceToFrame(g: Placement, w: number, h: number): (p: Point) => Point {
  const forward = frameToSource(g, w, h);
  return (target) => {
    let p: Point = [0.5, 0.5];
    const e = 1e-4;
    for (let i = 0; i < 12; i++) {
      const f = forward(p);
      const [rx, ry] = [f[0] - target[0], f[1] - target[1]];
      if (Math.abs(rx) < 1e-7 && Math.abs(ry) < 1e-7) break;
      const fx = forward([p[0] + e, p[1]]);
      const fy = forward([p[0], p[1] + e]);
      const [a, b, c, d] = [(fx[0] - f[0]) / e, (fy[0] - f[0]) / e, (fx[1] - f[1]) / e, (fy[1] - f[1]) / e];
      const det = a * d - b * c;
      if (Math.abs(det) < 1e-12) break;
      p = [p[0] - (d * rx - b * ry) / det, p[1] - (-c * rx + a * ry) / det];
    }
    return p;
  };
}

/** Index of the spot under source point `p` (the last drawn wins), or of its source
 *  circle when `sources` is set; null if none. `aspect` is width / height. */
export function spotAt(spots: readonly Spot[], p: Point, aspect: number, sources = false): number | null {
  const [sx, sy] = aspect >= 1 ? [1, 1 / aspect] : [aspect, 1];
  for (let i = spots.length - 1; i >= 0; i--) {
    const s = spots[i]!;
    const [cx, cy] = sources ? [s.sourceX, s.sourceY] : [s.x, s.y];
    // Fractions of the long edge, so the circle is round.
    if (Math.hypot((p[0] - cx) * sx, (p[1] - cy) * sy) <= s.radius) return i;
  }
  return null;
}

const round = (v: number) => Math.round(v * 1e5) / 1e5;

/** `spot` moved by `delta` (source fractions): the spot itself, or its source. */
export function moveSpot(spot: Spot, delta: Point, which: "spot" | "source"): Spot {
  const clamp = (v: number) => round(Math.min(1, Math.max(0, v)));
  return which === "spot"
    ? { ...spot, x: clamp(spot.x + delta[0]), y: clamp(spot.y + delta[1]) }
    : { ...spot, sourceX: clamp(spot.sourceX + delta[0]), sourceY: clamp(spot.sourceY + delta[1]) };
}

/** The Remove brush's soft edge (ADR 0066): enough to blend the fill in. */
export const REMOVE_FEATHER = 10;

export function removalsOf(r: EditRecipe): Removal[] {
  return r.removals ?? [];
}

/** `r` with `removals`, left out when there are none (as the renderer writes it). */
export function withRemovals(r: EditRecipe, removals: Removal[]): EditRecipe {
  const { removals: _, ...rest } = r;
  return removals.length > 0 ? { ...rest, removals } : rest;
}

/** A source photo's diagonal in units of its long edge (strokes are sized by the
 *  diagonal, spots by the long edge). `aspect` is width / height. */
function diagonalInLongEdges(aspect: number): number {
  return aspect >= 1 ? Math.hypot(1, 1 / aspect) : Math.hypot(aspect, 1);
}

/** The Remove stroke size (a fraction of the photo's diagonal) for a brush `px` across
 *  on screen, where `scale` is screen pixels per photo long edge. */
export function strokeSizeFor(px: number, scale: number, aspect: number): number {
  return Math.round(((px / 2 / scale) / diagonalInLongEdges(aspect)) * 1e6) / 1e6;
}

/** A Remove stroke's radius in fractions of the photo's long edge. */
export function strokeRadius(stroke: Stroke, aspect: number): number {
  return stroke.size * diagonalInLongEdges(aspect);
}

/** A new removal: one stroke along `points` (source fractions), `size` a fraction of
 *  the photo's diagonal. */
export function newRemoval(points: readonly Point[], size: number): Removal {
  const round = (v: number) => Math.round(v * 10_000) / 10_000;
  return {
    strokes: [{ size, feather: REMOVE_FEATHER, flow: 100, points: points.map(([x, y]) => [round(x), round(y)]) }],
  };
}

/** Index of the removal whose painted area covers source point `p` (the last made
 *  wins), or null. `aspect` is width / height. */
export function removalAt(removals: readonly Removal[], p: Point, aspect: number): number | null {
  const [sx, sy] = aspect >= 1 ? [1, 1 / aspect] : [aspect, 1];
  // In fractions of the long edge, so distances are round.
  const q: Point = [p[0] * sx, p[1] * sy];
  for (let i = removals.length - 1; i >= 0; i--) {
    for (const s of removals[i]!.strokes) {
      if (s.erase) continue;
      const r = strokeRadius(s, aspect);
      const pts = s.points.map(([x, y]): Point => [x * sx, y * sy]);
      const segments = pts.length === 1 ? [[pts[0]!, pts[0]!]] : pts.slice(1).map((b, k) => [pts[k]!, b]);
      if (segments.some(([a, b]) => distanceToSegment(q, a!, b!) <= r)) return i;
    }
  }
  return null;
}

function distanceToSegment(p: Point, a: Point, b: Point): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const len2 = dx * dx + dy * dy;
  const t = len2 > 0 ? Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2)) : 0;
  return Math.hypot(a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
}

/** The dust spots found (ADR 0058) whose centres no spot in `spots` covers yet: the
 *  ones still to fix. `aspect` is width / height. */
export function stillToFix(found: readonly Spot[], spots: readonly Spot[], aspect: number): Spot[] {
  return found.filter((d) => spotAt(spots, [d.x, d.y], aspect) === null);
}

/** `spots` without those in `fixed` (the same spot: centre and size). */
export function withoutSpots(spots: readonly Spot[], fixed: readonly Spot[]): Spot[] {
  const same = (a: Spot, b: Spot) => a.x === b.x && a.y === b.y && a.radius === b.radius;
  return spots.filter((s) => !fixed.some((f) => same(s, f)));
}
