import type { AspectRatio } from "../../ipc/generated/AspectRatio";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { Geometry } from "../../ipc/generated/Geometry";

/** What turns the photo before it is cropped: straighten and perspective. */
export type ViewShape = Pick<Geometry, "straighten" | "vertical" | "horizontal">;

/**
 * Crop rectangle maths for the crop tool (ADR 0032). Rectangles are fractions of the
 * photo's width and height in the straightened view, as in the recipe. The renderer
 * owns the geometry (`renderer::geometry`); this is the interaction side: fitting a
 * shape, and moving rectangles between the photo and the crop view on screen.
 * Perspective (ADR 0034) follows the renderer's `Mapping`.
 */

export const FULL: CropRect = { x: 0, y: 0, w: 1, h: 1 };
const MIN_SIDE = 0.02;

export const ASPECTS: ReadonlyArray<{ id: AspectRatio; label: string }> = [
  { id: "original", label: "Original" },
  { id: "free", label: "Free" },
  { id: "square", label: "1:1" },
  { id: "portrait4x5", label: "4:5" },
  { id: "wide16x9", label: "16:9" },
];

/** Width / height in pixels for a photo of `w` x `h`, or null (free). */
export function aspectRatioOf(a: AspectRatio, w: number, h: number): number | null {
  switch (a) {
    case "original":
      return w / h;
    case "free":
      return null;
    case "square":
      return 1;
    case "portrait4x5":
      return 0.8;
    case "wide16x9":
      return 16 / 9;
  }
}

/**
 * The largest rectangle of pixel `ratio` centred in the view that fits inside the photo
 * rotated by `straighten` degrees. Mirrors `renderer::geometry::fit_crop` (a shared
 * test case pins them together).
 */
export function fitCrop(ratio: number, straighten: number, w: number, h: number): CropRect {
  const t = (Math.abs(Math.max(-15, Math.min(15, straighten))) * Math.PI) / 180;
  const [s, c] = [Math.sin(t), Math.cos(t)];
  const a = Math.min((0.5 * w) / (c + s / ratio), (0.5 * h) / (s + c / ratio));
  const cw = (2 * a) / w;
  const ch = (2 * a) / ratio / h;
  return { x: 0.5 - cw / 2, y: 0.5 - ch / 2, w: cw, h: ch };
}

/** Perspective at ±100 turns the virtual camera this many degrees. */
const MAX_PERSPECTIVE_DEGREES = 20;

/**
 * Where view point (`vx`, `vy`) (photo fractions) comes from in the photo (pixels):
 * straighten undone, then the perspective homography, centred. Mirrors the renderer's
 * `Mapping::source`.
 */
export function viewToSource(g: ViewShape, w: number, h: number): (vx: number, vy: number) => [number, number] {
  const t = (clamp(g.straighten, -15, 15) * Math.PI) / 180;
  const [sin, cos] = [Math.sin(t), Math.cos(t)];
  const focal = Math.max(w, h);
  const a = (-clamp(g.vertical, -100, 100) / 100) * MAX_PERSPECTIVE_DEGREES * (Math.PI / 180);
  const b = (-clamp(g.horizontal, -100, 100) / 100) * MAX_PERSPECTIVE_DEGREES * (Math.PI / 180);
  const [sa, ca, sb, cb] = [Math.sin(a), Math.cos(a), Math.sin(b), Math.cos(b)];
  // Rx(a) * Ry(b), row-major.
  const r = [cb, 0, sb, sa * sb, ca, -sa * cb, -ca * sb, sa, ca * cb] as const;
  const [cx, cy] = [r[2] / r[8], r[5] / r[8]];
  const perspective = g.vertical !== 0 || g.horizontal !== 0;
  return (vx, vy) => {
    const [dx, dy] = [(vx - 0.5) * w, (vy - 0.5) * h];
    let [px, py] = [dx * cos - dy * sin, dx * sin + dy * cos];
    if (perspective) {
      const [u, v] = [px / focal, py / focal];
      const z = Math.max(r[6] * u + r[7] * v + r[8], 1e-3);
      px = ((r[0] * u + r[1] * v + r[2]) / z - cx) * focal;
      py = ((r[3] * u + r[4] * v + r[5]) / z - cy) * focal;
    }
    return [w / 2 + px, h / 2 + py];
  };
}

/** Whether rectangle `c` of the view lies inside the photo. */
function contains(map: (vx: number, vy: number) => [number, number], c: CropRect, w: number, h: number): boolean {
  const corners: Array<[number, number]> = [
    [c.x, c.y],
    [c.x + c.w, c.y],
    [c.x, c.y + c.h],
    [c.x + c.w, c.y + c.h],
  ];
  return corners.every(([vx, vy]) => {
    const [sx, sy] = map(vx, vy);
    return sx >= -1e-3 && sx <= w + 1e-3 && sy >= -1e-3 && sy <= h + 1e-3;
  });
}

/**
 * The largest rectangle of pixel `ratio` centred in the view that fits inside the photo
 * after straighten and perspective. Mirrors `renderer::geometry::fit_crop_for`.
 */
export function fitCropFor(ratio: number, g: ViewShape, w: number, h: number): CropRect {
  if (g.vertical === 0 && g.horizontal === 0) return fitCrop(ratio, g.straighten, w, h);
  const map = viewToSource(g, w, h);
  const rect = (a: number): CropRect => {
    const [cw, ch] = [(2 * a) / w, (2 * a) / ratio / h];
    return { x: 0.5 - cw / 2, y: 0.5 - ch / 2, w: cw, h: ch };
  };
  let [lo, hi] = [0, 0.5 * Math.max(w, h * ratio)];
  for (let i = 0; i < 40; i++) {
    const mid = 0.5 * (lo + hi);
    if (contains(map, rect(mid), w, h)) lo = mid;
    else hi = mid;
  }
  return rect(lo);
}

/** What the crop tool shows: the largest area of the straightened, corrected photo in
 *  its own shape (so no empty corners). */
export function cropView(g: ViewShape, w: number, h: number): CropRect {
  return fitCropFor(w / h, g, w, h);
}

/** `c` (photo fractions) as fractions of the view `v`. */
export function toView(c: CropRect, v: CropRect): CropRect {
  return { x: (c.x - v.x) / v.w, y: (c.y - v.y) / v.h, w: c.w / v.w, h: c.h / v.h };
}

/** `o` (fractions of the view `v`) as photo fractions. */
export function fromView(o: CropRect, v: CropRect): CropRect {
  return { x: v.x + o.x * v.w, y: v.y + o.y * v.h, w: o.w * v.w, h: o.h * v.h };
}

/** A crop kept in the same place relative to the view when the view changes (the
 *  straighten angle or perspective moved). */
export function remap(c: CropRect, from: CropRect, to: CropRect): CropRect {
  return fromView(toView(c, from), to);
}

/** The ratio of `pixelRatio` in view fractions, for a view `v` of a `w` x `h` photo. */
export function viewRatio(pixelRatio: number, v: CropRect, w: number, h: number): number {
  return (pixelRatio * (v.h * h)) / (v.w * w);
}

/** The largest rectangle of view ratio `r`, centred in the view (view fractions). */
export function largestIn(r: number): CropRect {
  const [cw, ch] = r >= 1 ? [1, 1 / r] : [r, 1];
  return { x: (1 - cw) / 2, y: (1 - ch) / 2, w: cw, h: ch };
}

export type Handle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

/**
 * `start` (view fractions) after dragging `handle` by (`dx`, `dy`) view fractions,
 * kept inside the view, at least a minimum size, and at view ratio `r` if locked. The
 * opposite corner or edge stays put.
 */
export function drag(start: CropRect, handle: Handle, dx: number, dy: number, r: number | null): CropRect {
  if (handle === "move") {
    return {
      ...start,
      x: clamp(start.x + dx, 0, 1 - start.w),
      y: clamp(start.y + dy, 0, 1 - start.h),
    };
  }
  let [left, top, right, bottom] = [start.x, start.y, start.x + start.w, start.y + start.h];
  if (handle.includes("w")) left = clamp(left + dx, 0, right - MIN_SIDE);
  if (handle.includes("e")) right = clamp(right + dx, left + MIN_SIDE, 1);
  if (handle.includes("n")) top = clamp(top + dy, 0, bottom - MIN_SIDE);
  if (handle.includes("s")) bottom = clamp(bottom + dy, top + MIN_SIDE, 1);
  if (r === null) return { x: left, y: top, w: right - left, h: bottom - top };

  // Locked shape: the side that moved sets the size, about the fixed anchor.
  const horizontal = handle === "e" || handle === "w";
  const vertical = handle === "n" || handle === "s";
  let w = right - left;
  let h = bottom - top;
  if (horizontal) h = w / r;
  else if (vertical) w = h * r;
  else if (w / r > h) h = w / r;
  else w = h * r;
  // Anchor: the fixed corner or the middle of the fixed edge.
  const ax = handle.includes("w") ? start.x + start.w : handle.includes("e") ? start.x : start.x + start.w / 2;
  const ay = handle.includes("n") ? start.y + start.h : handle.includes("s") ? start.y : start.y + start.h / 2;
  const fx = handle.includes("w") ? 1 : handle.includes("e") ? 0 : 0.5;
  const fy = handle.includes("n") ? 1 : handle.includes("s") ? 0 : 0.5;
  // Largest scale that keeps the rectangle inside the view.
  const room = (anchor: number, f: number, size: number) =>
    f === 1 ? anchor / size : f === 0 ? (1 - anchor) / size : Math.min(anchor, 1 - anchor) / (size / 2);
  const s = Math.min(1, room(ax, fx, w), room(ay, fy, h));
  w *= s;
  h *= s;
  return { x: ax - fx * w, y: ay - fy * h, w, h };
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}
