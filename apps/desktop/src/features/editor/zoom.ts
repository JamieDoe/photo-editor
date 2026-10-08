/**
 * Zoom (ADR 0070): the photo larger than Fit, from just above it to 800 %, panned by
 * its centre. A zoom's scale is device pixels per full-resolution pixel: 1 is 100 %.
 * Only geometry here; Rust renders the visible window.
 */

export interface Size {
  width: number;
  height: number;
}

/** Where the zoomed view is centred, as fractions of the photo's output. */
export interface ZoomCentre {
  x: number;
  y: number;
}

/** A zoom: where it is centred, and its scale (device pixels per full-resolution
 *  pixel; 1 is 100 %). */
export interface Zoom {
  centre: ZoomCentre;
  scale: number;
}

/** The most the photo is enlarged: 800 %. */
export const MAX_SCALE = 8;

/** Zoom levels ⌘+ and ⌘− step through, above Fit. */
export const ZOOM_STEPS = [1 / 8, 1 / 4, 1 / 3, 1 / 2, 2 / 3, 1, 2, 4, 8];

/** A window of the full-resolution output, in its pixels: x, y, width, height. */
export type OutputWindow = [number, number, number, number];

export interface ZoomLayout {
  /** The photo's box at the zoom, in CSS pixels relative to the viewport. */
  left: number;
  top: number;
  width: number;
  height: number;
  /** The part of the output the viewport shows, in full-resolution pixels. */
  window: OutputWindow;
}

/** One axis: the box's offset in the viewport. Smaller than the viewport, the photo is
 *  centred; larger, `centre` is kept in the middle without uncovering an edge. */
function offset(view: number, box: number, centre: number, dpr: number): number {
  const raw = box <= view ? (view - box) / 2 : Math.min(0, Math.max(view - box, view / 2 - centre * box));
  // Whole device pixels, so a full-resolution window lands pixel for pixel.
  return Math.round(raw * dpr) / dpr;
}

/** The visible span of one axis, in output pixels: [start, length]. `perPixel` is CSS
 *  pixels per output pixel. */
function span(view: number, box: number, at: number, perPixel: number, full: number): [number, number] {
  const start = Math.max(0, Math.floor(Math.max(0, -at) / perPixel));
  const end = Math.min(full, Math.ceil(Math.min(box, view - at) / perPixel));
  return [start, Math.max(1, end - start)];
}

/** The box's CSS size at `scale` for an output of `full` pixels. */
const boxSize = (full: Size, dpr: number, scale: number): Size => ({ width: (full.width * scale) / dpr, height: (full.height * scale) / dpr });

/** The layout at `zoom` in a viewport of `view` CSS pixels, at `dpr` device pixels
 *  each, for an output of `full` pixels. */
export function zoomLayout(view: Size, full: Size, dpr: number, zoom: Zoom): ZoomLayout {
  const { width, height } = boxSize(full, dpr, zoom.scale);
  const left = offset(view.width, width, zoom.centre.x, dpr);
  const top = offset(view.height, height, zoom.centre.y, dpr);
  const perPixel = zoom.scale / dpr;
  const [x, w] = span(view.width, width, left, perPixel, full.width);
  const [y, h] = span(view.height, height, top, perPixel, full.height);
  return { left, top, width, height, window: [x, y, w, h] };
}

/** The scale at which the photo fits the view (as the viewer shows it at Fit). */
export function fitScale(view: Size, full: Size, dpr: number): number {
  if (full.width <= 0 || full.height <= 0) return 1;
  return Math.min(view.width / full.width, view.height / full.height) * dpr;
}

/** `centre` kept where the view stays covered at `scale` (centred when the photo is
 *  smaller). */
export function clampCentre(view: Size, full: Size, dpr: number, centre: ZoomCentre, scale: number): ZoomCentre {
  const box = boxSize(full, dpr, scale);
  const axis = (v: number, b: number, c: number) => {
    if (b <= v) return 0.5;
    const half = v / 2 / b;
    return Math.min(1 - half, Math.max(half, c));
  };
  return { x: axis(view.width, box.width, centre.x), y: axis(view.height, box.height, centre.y) };
}

/** The zoom at `scale` that puts the photo's point `at` (fractions) under the viewport
 *  point `pointer` (CSS pixels): zooming on the spot clicked or pinched. */
export function zoomKeeping(view: Size, full: Size, dpr: number, at: ZoomCentre, pointer: { x: number; y: number }, scale: number): Zoom {
  const box = boxSize(full, dpr, scale);
  const centre = { x: at.x + (view.width / 2 - pointer.x) / box.width, y: at.y + (view.height / 2 - pointer.y) / box.height };
  return { centre: clampCentre(view, full, dpr, centre, scale), scale };
}

/** The photo's point under the viewport point `pointer` at `zoom`, as fractions. */
export function pointAt(view: Size, full: Size, dpr: number, zoom: Zoom, pointer: { x: number; y: number }): ZoomCentre {
  const z = zoomLayout(view, full, dpr, zoom);
  return { x: (pointer.x - z.left) / z.width, y: (pointer.y - z.top) / z.height };
}

/** `zoom` moved by a drag or scroll of `dx`, `dy` CSS pixels (the photo follows a
 *  drag: dragging right shows more of the left). */
export function panned(view: Size, full: Size, dpr: number, zoom: Zoom, dx: number, dy: number): Zoom {
  const box = boxSize(full, dpr, zoom.scale);
  const centre = { x: zoom.centre.x - dx / box.width, y: zoom.centre.y - dy / box.height };
  return { centre: clampCentre(view, full, dpr, centre, zoom.scale), scale: zoom.scale };
}

/** The next zoom level from `scale` in `direction` (+1 in, -1 out), or null for Fit:
 *  the steps above Fit, Fit itself below them. */
export function steppedScale(scale: number, direction: 1 | -1, fit: number): number | null {
  const above = ZOOM_STEPS.filter((s) => s > fit * 1.01);
  if (direction > 0) return above.find((s) => s > scale * 1.01) ?? null;
  const below = above.filter((s) => s < scale * 0.99);
  return below.length > 0 ? below[below.length - 1]! : null;
}

/** A zoom's level in percent, as the toolbar shows it. */
export const zoomPercent = (scale: number) => `${Math.round(scale * 100)}%`;

/** Whether a zoom where the whole output's long edge is `zoomedLongEdge` (of
 *  `outputLong` at full resolution) needs more than the largest preview level (of a
 *  photo of `photo` pixels) holds: as the engine picks the source a window renders
 *  from, so the full resolution is decoded only when it will be used. */
export function needsFull(level: [number, number], photo: [number, number], zoomedLongEdge: number, outputLong: number): boolean {
  if (outputLong <= 0) return true;
  return (zoomedLongEdge / outputLong) * Math.max(...photo) > Math.max(...level);
}

export const sameWindow = (a: OutputWindow | null, b: OutputWindow | null) =>
  a === b || (a !== null && b !== null && a.every((v, i) => v === b[i]));

/** A part of the photo's box, as fractions of it: left, top, right, bottom. */
export type BoxPart = [number, number, number, number];

/** The part of the box the viewport shows at 100 %. */
export function visiblePart(view: Size, layout: ZoomLayout): BoxPart {
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  return [
    clamp(-layout.left / layout.width),
    clamp(-layout.top / layout.height),
    clamp((view.width - layout.left) / layout.width),
    clamp((view.height - layout.top) / layout.height),
  ];
}

/** About how many steps a drawn part snaps to per visible span: small pans reuse what
 *  is drawn. */
const PART_STEPS = 8;

/** The part of the box an overlay draws for `visible`: grown by a step on each side
 *  and snapped to steps of its span, so panning redraws it only every so often. */
export function drawnPart(visible: BoxPart): BoxPart {
  const axis = (a: number, b: number): [number, number] => {
    // A power of two at most an eighth of the span: exact in floating point, so the
    // same pan position always gives the same part.
    const step = Math.max((b - a) / PART_STEPS, 1 / 4096);
    const grid = 2 ** Math.floor(Math.log2(step));
    return [Math.max(0, Math.floor(a / grid) * grid - grid), Math.min(1, Math.ceil(b / grid) * grid + grid)];
  };
  const [x0, x1] = axis(visible[0], visible[2]);
  const [y0, y1] = axis(visible[1], visible[3]);
  return [x0, y0, x1, y1];
}
