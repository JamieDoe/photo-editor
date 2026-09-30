import type { CropRect } from "../../ipc/generated/CropRect";
import type { Stroke } from "../../ipc/generated/Stroke";
import { toShown } from "./masks";

/**
 * A brush shape's coverage on a canvas (ADR 0042), drawn as the renderer composes it;
 * `MaskTint` colours it and combines it with the mask's other shapes. Each stroke is
 * its profile at its flow; paint is drawn over what is there and an erase stroke cuts
 * it away ("destination-out"), so an erase takes away only what came before. Finished
 * strokes are kept in a cached layer: while painting, only the stroke in progress is
 * drawn, however many came before.
 */

/** Rings a stroke's soft edge is drawn with. */
const EDGE_STEPS = 10;

/**
 * The widths and opacities of nested round-capped lines whose stacked opacity is the
 * renderer's brush profile: full within the unfeathered radius, smoothstep to nothing
 * at the radius. A line of half-width `d` covers every point within `d` of the stroke,
 * so ring k (from the outside in) adds its opacity inside distance `d`; the opacities
 * are chosen so the total over each ring's band is the profile at the band's middle.
 */
export function edgeRings(r: number, feather: number): Array<{ halfWidth: number; opacity: number }> {
  const inner = r * (1 - feather / 100);
  if (r - inner < 0.5) return [{ halfWidth: r, opacity: 1 }];
  const profile = (d: number) => {
    const t = Math.min(1, Math.max(0, (d - inner) / (r - inner)));
    return 1 - t * t * (3 - 2 * t);
  };
  const rings: Array<{ halfWidth: number; opacity: number }> = [];
  // Outermost first. `covered` is the stacked opacity of the rings drawn so far.
  let covered = 0;
  for (let k = EDGE_STEPS; k >= 1; k--) {
    const outer = inner + ((r - inner) * k) / EDGE_STEPS;
    const target = profile(inner + ((r - inner) * (k - 0.5)) / EDGE_STEPS);
    const opacity = covered >= 1 ? 0 : Math.max(0, (target - covered) / (1 - covered));
    rings.push({ halfWidth: outer, opacity });
    covered = covered + (1 - covered) * opacity;
  }
  rings.push({ halfWidth: inner, opacity: 1 });
  return rings.filter((ring) => ring.halfWidth > 0 && ring.opacity > 0);
}

/** The canvas the tint is drawn on: its pixels, and the crop it shows of the frame. */
export interface TintView {
  width: number;
  height: number;
  crop: CropRect;
}

export function canvas2d(c: HTMLCanvasElement): CanvasRenderingContext2D {
  const ctx = c.getContext("2d");
  if (!ctx) throw new Error("no 2D canvas");
  return ctx;
}

/** Sizes `c` to `width` x `height` (which clears it) if it is not already. */
export function fit(c: HTMLCanvasElement, width: number, height: number): void {
  if (c.width !== width || c.height !== height) {
    c.width = width;
    c.height = height;
  }
}

export const viewKey = (v: TintView) => `${v.width}x${v.height}:${v.crop.x},${v.crop.y},${v.crop.w},${v.crop.h}`;

/** A brush shape's coverage as a canvas's alpha. */
export class BrushTint {
  /** Finished strokes (all but the last), and which they are. */
  private readonly base = document.createElement("canvas");
  private baseStrokes: readonly Stroke[] = [];
  private baseView = "";
  /** The base with the last stroke over it, and what it was drawn from. */
  private readonly cover = document.createElement("canvas");
  private coverStrokes: readonly Stroke[] | null = null;
  private coverView = "";
  /** One stroke's profile before it is composed. */
  private readonly scratch = document.createElement("canvas");

  /** `strokes`' coverage, as alpha (kept until the next call). */
  coverage(strokes: readonly Stroke[], view: TintView): HTMLCanvasElement {
    const key = viewKey(view);
    if (strokes === this.coverStrokes && key === this.coverView) return this.cover;
    const { width, height } = view;
    for (const c of [this.base, this.cover, this.scratch]) fit(c, width, height);
    const finished = strokes.slice(0, -1);
    const last = strokes.at(-1);
    // The cached layer: extended when the finished strokes only grew, else redrawn.
    const extends_ =
      key === this.baseView &&
      this.baseStrokes.length <= finished.length &&
      this.baseStrokes.every((s, i) => s === finished[i]);
    const base = canvas2d(this.base);
    if (!extends_) base.clearRect(0, 0, width, height);
    for (const s of finished.slice(extends_ ? this.baseStrokes.length : 0)) this.compose(base, s, view);
    this.baseStrokes = finished;
    this.baseView = key;

    const cover = canvas2d(this.cover);
    cover.clearRect(0, 0, width, height);
    cover.drawImage(this.base, 0, 0);
    if (last) this.compose(cover, last, view);
    this.coverStrokes = strokes;
    this.coverView = key;
    return this.cover;
  }

  /** Stroke `s` drawn onto `ctx` as the renderer composes it. */
  private compose(ctx: CanvasRenderingContext2D, s: Stroke, view: TintView): void {
    if (s.points.length === 0 || s.flow <= 0) return;
    const { width, height, crop } = view;
    const diagonal = Math.hypot(width / crop.w, height / crop.h);
    const r = s.size * diagonal;
    const pts = s.points.map((p) => {
      const q = toShown(p, crop);
      return [q[0] * width, q[1] * height] as const;
    });
    // Only the stroke's own area is cleared and copied.
    const xs = pts.map((p) => p[0]);
    const ys = pts.map((p) => p[1]);
    const x0 = Math.max(0, Math.floor(Math.min(...xs) - r - 1));
    const y0 = Math.max(0, Math.floor(Math.min(...ys) - r - 1));
    const x1 = Math.min(width, Math.ceil(Math.max(...xs) + r + 1));
    const y1 = Math.min(height, Math.ceil(Math.max(...ys) + r + 1));
    if (x1 <= x0 || y1 <= y0) return;
    const path = new Path2D();
    path.moveTo(pts[0]![0], pts[0]![1]);
    // A single point: a tiny line whose round caps make the dab.
    if (pts.length === 1) path.lineTo(pts[0]![0] + 0.01, pts[0]![1]);
    for (const p of pts.slice(1)) path.lineTo(p[0], p[1]);
    const scratch = canvas2d(this.scratch);
    scratch.save();
    scratch.clearRect(x0, y0, x1 - x0, y1 - y0);
    scratch.strokeStyle = "#000";
    scratch.lineCap = "round";
    scratch.lineJoin = "round";
    for (const ring of edgeRings(r, s.feather)) {
      scratch.globalAlpha = ring.opacity;
      scratch.lineWidth = 2 * ring.halfWidth;
      scratch.stroke(path);
    }
    scratch.restore();
    ctx.save();
    ctx.globalCompositeOperation = s.erase ? "destination-out" : "source-over";
    ctx.globalAlpha = s.flow / 100;
    ctx.drawImage(this.scratch, x0, y0, x1 - x0, y1 - y0, x0, y0, x1 - x0, y1 - y0);
    ctx.restore();
  }
}
