import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Geometry } from "../../ipc/generated/Geometry";
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

/** The retouch tools, as in the design. Remove (paint to erase) is not built yet. */
export const RETOUCH_TOOLS: ReadonlyArray<{ kind: SpotKind; label: string; hint: string; text: string }> = [
  { kind: "heal", label: "Heal", hint: "Blend texture from a nearby area", text: "Blends texture and tone from a nearby area. Good for blemishes and small marks." },
  {
    kind: "clone",
    label: "Clone",
    hint: "Copy pixels exactly",
    text: "Copies pixels exactly from a nearby area. Drag the source circle, or Option-click, to choose where from.",
  },
];

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
