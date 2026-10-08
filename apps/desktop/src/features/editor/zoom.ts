/**
 * Zoom (ADR 0070): the photo at 100 %, one of its full-resolution pixels to one of
 * the screen's, panned by its centre. Only geometry here; Rust renders the visible
 * window.
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

/** A window of the full-resolution output, in its pixels: x, y, width, height. */
export type OutputWindow = [number, number, number, number];

export interface ZoomLayout {
  /** The photo's box at 100 %, in CSS pixels relative to the viewport. */
  left: number;
  top: number;
  width: number;
  height: number;
  /** The part of the output the viewport shows. */
  window: OutputWindow;
}

/** One axis: the box's offset in the viewport. Smaller than the viewport, the photo is
 *  centred; larger, `centre` is kept in the middle without uncovering an edge. */
function offset(view: number, box: number, centre: number, dpr: number): number {
  const raw = box <= view ? (view - box) / 2 : Math.min(0, Math.max(view - box, view / 2 - centre * box));
  // Whole device pixels, so a full-resolution window lands pixel for pixel.
  return Math.round(raw * dpr) / dpr;
}

/** The visible span of one axis, in output pixels: [start, length]. */
function span(view: number, box: number, at: number, dpr: number, full: number): [number, number] {
  const start = Math.max(0, Math.floor(Math.max(0, -at) * dpr));
  const end = Math.min(full, Math.ceil(Math.min(box, view - at) * dpr));
  return [start, Math.max(1, end - start)];
}

/** The layout at 100 % in a viewport of `view` CSS pixels, at `dpr` device pixels each,
 *  for an output of `full` pixels centred at `centre`. */
export function zoomLayout(view: Size, full: Size, dpr: number, centre: ZoomCentre): ZoomLayout {
  const width = full.width / dpr;
  const height = full.height / dpr;
  const left = offset(view.width, width, centre.x, dpr);
  const top = offset(view.height, height, centre.y, dpr);
  const [x, w] = span(view.width, width, left, dpr, full.width);
  const [y, h] = span(view.height, height, top, dpr, full.height);
  return { left, top, width, height, window: [x, y, w, h] };
}

/** `centre` kept where the view stays covered (centred when the photo is smaller). */
export function clampCentre(view: Size, full: Size, dpr: number, centre: ZoomCentre): ZoomCentre {
  const axis = (v: number, box: number, c: number) => {
    if (box <= v) return 0.5;
    const half = v / 2 / box;
    return Math.min(1 - half, Math.max(half, c));
  };
  return { x: axis(view.width, full.width / dpr, centre.x), y: axis(view.height, full.height / dpr, centre.y) };
}

/** The centre that puts the photo's point `at` (fractions) under the viewport point
 *  `pointer` (CSS pixels) at 100 %: zooming in on the spot clicked. */
export function centreKeeping(view: Size, full: Size, dpr: number, at: ZoomCentre, pointer: { x: number; y: number }): ZoomCentre {
  const width = full.width / dpr;
  const height = full.height / dpr;
  const centre = { x: at.x + (view.width / 2 - pointer.x) / width, y: at.y + (view.height / 2 - pointer.y) / height };
  return clampCentre(view, full, dpr, centre);
}

/** `centre` moved by a drag or scroll of `dx`, `dy` CSS pixels (the photo follows a
 *  drag: dragging right shows more of the left). */
export function panned(view: Size, full: Size, dpr: number, centre: ZoomCentre, dx: number, dy: number): ZoomCentre {
  const width = full.width / dpr;
  const height = full.height / dpr;
  return clampCentre(view, full, dpr, { x: centre.x - dx / width, y: centre.y - dy / height });
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
