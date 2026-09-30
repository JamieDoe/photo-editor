/**
 * The tone curve's maths (ADR 0037): evaluating the photographer's points as the
 * renderer does (`renderer::ops::point_curve`, pinned by a shared test case), and the
 * edits the graph makes to them. Points are `[input, output]` display tones, 0..1,
 * sorted by input.
 */

export type CurvePoint = [number, number];

export const DIAGONAL: readonly CurvePoint[] = [
  [0, 0],
  [1, 1],
];
/** As the renderer keeps them. */
export const MAX_POINTS = 16;
export const MIN_GAP = 0.01;

const round = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 10_000) / 10_000;

/** Every point on the diagonal from (0, 0) to (1, 1): the curve changes nothing. */
export function isDiagonal(points: readonly CurvePoint[]): boolean {
  const first = points[0];
  const last = points[points.length - 1];
  return (
    first !== undefined &&
    last !== undefined &&
    first[0] === 0 &&
    first[1] === 0 &&
    last[0] === 1 &&
    last[1] === 1 &&
    points.every(([x, y]) => Math.abs(x - y) < 1e-6)
  );
}

/** PCHIP tangents, as the renderer computes them. */
function tangents(p: readonly CurvePoint[]): number[] {
  const n = p.length;
  const h = (i: number) => p[i + 1]![0] - p[i]![0];
  const d = (i: number) => (p[i + 1]![1] - p[i]![1]) / h(i);
  const m = new Array<number>(n).fill(0);
  m[0] = d(0);
  m[n - 1] = d(n - 2);
  for (let i = 1; i < n - 1; i++) {
    const d0 = d(i - 1);
    const d1 = d(i);
    if (d0 * d1 <= 0) continue;
    const h0 = h(i - 1);
    const h1 = h(i);
    m[i] = (3 * (h0 + h1)) / ((2 * h1 + h0) / d0 + (h1 + 2 * h0) / d1);
  }
  return m;
}

/** The output tone for input `x`: a monotone cubic through the points, flat beyond
 *  the first and last. */
export function evalCurve(points: readonly CurvePoint[], x: number): number {
  const p = points;
  const n = p.length;
  const first = p[0]!;
  const last = p[n - 1]!;
  if (x <= first[0]) return first[1];
  if (x >= last[0]) return last[1];
  let i = 0;
  while (p[i + 1]![0] <= x) i++;
  const m = tangents(p);
  const [x0, y0] = p[i]!;
  const [x1, y1] = p[i + 1]!;
  const h = x1 - x0;
  const t = (x - x0) / h;
  const t2 = t * t;
  const t3 = t2 * t;
  const y =
    (2 * t3 - 3 * t2 + 1) * y0 + (t3 - 2 * t2 + t) * h * m[i]! + (-2 * t3 + 3 * t2) * y1 + (t3 - t2) * h * m[i + 1]!;
  return Math.min(1, Math.max(0, y));
}

/** An SVG path of the curve on a `w` x `h` graph (y up). */
export function curvePath(points: readonly CurvePoint[], w: number, h: number, samples = 96): string {
  let d = "";
  for (let k = 0; k <= samples; k++) {
    const x = k / samples;
    const y = evalCurve(points, x);
    d += `${k ? " L" : "M"}${(x * w).toFixed(2)} ${(h - y * h).toFixed(2)}`;
  }
  return d;
}

/** The point nearest (`x`, `y`) within `rx` horizontally and `ry` vertically (both in
 *  graph fractions), or null. */
export function nearestPoint(points: readonly CurvePoint[], x: number, y: number, rx: number, ry: number): number | null {
  let best: number | null = null;
  let bestDistance = Infinity;
  points.forEach(([px, py], i) => {
    const dx = (px - x) / rx;
    const dy = (py - y) / ry;
    const distance = dx * dx + dy * dy;
    if (distance <= 1 && distance < bestDistance) {
      best = i;
      bestDistance = distance;
    }
  });
  return best;
}

/** A new point on the curve at input `x`: the points and its index, or null when
 *  there is no room (too close to another point, or the most points already). */
export function insertPoint(points: readonly CurvePoint[], x: number): { points: CurvePoint[]; index: number } | null {
  const rx = round(x);
  if (points.length >= MAX_POINTS || points.some(([px]) => Math.abs(px - rx) < MIN_GAP)) return null;
  const point: CurvePoint = [rx, round(evalCurve(points, rx))];
  const index = points.findIndex(([px]) => px > rx);
  const at = index === -1 ? points.length : index;
  return { points: [...points.slice(0, at), point, ...points.slice(at)], index: at };
}

/** Point `i` moved to (`x`, `y`), kept between its neighbours (at least `MIN_GAP`
 *  apart) and within the graph. */
export function movePoint(points: readonly CurvePoint[], i: number, x: number, y: number): CurvePoint[] {
  const lo = i > 0 ? points[i - 1]![0] + MIN_GAP : 0;
  const hi = i < points.length - 1 ? points[i + 1]![0] - MIN_GAP : 1;
  const next = points.slice() as CurvePoint[];
  next[i] = [round(Math.min(hi, Math.max(lo, x))), round(y)];
  return next;
}

/** Without point `i`. The two end points stay (a curve needs both). */
export function removePoint(points: readonly CurvePoint[], i: number): CurvePoint[] {
  if (i <= 0 || i >= points.length - 1) return points.slice() as CurvePoint[];
  return points.filter((_, k) => k !== i);
}

/** An end point back at its corner. */
export function resetEnd(points: readonly CurvePoint[], i: number): CurvePoint[] {
  const next = points.slice() as CurvePoint[];
  if (i === 0) next[0] = [0, 0];
  else if (i === points.length - 1) next[i] = [1, 1];
  return next;
}

/** A display tone as the graph reads it out: 0..255. */
export function toneValue(v: number): number {
  return Math.round(v * 255);
}
