import type { ColourGrading } from "../../ipc/generated/ColourGrading";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { GradeWheel } from "../../ipc/generated/GradeWheel";

/** Colour grading's ranges (ADR 0052), in the panel's order. */
export const GRADE_RANGES = [
  { id: "shadows", label: "Shadows" },
  { id: "midtones", label: "Midtones" },
  { id: "highlights", label: "Highlights" },
  { id: "global", label: "Global" },
] as const;

export type GradeRange = (typeof GRADE_RANGES)[number]["id"];

const NEUTRAL_WHEEL: GradeWheel = { hue: 0, saturation: 0, luminance: 0 };

/** The recipe's grading, or its neutral form. */
export function gradingOf(r: EditRecipe): ColourGrading {
  return (
    r.colourGrading ?? {
      shadows: NEUTRAL_WHEEL,
      midtones: NEUTRAL_WHEEL,
      highlights: NEUTRAL_WHEEL,
      global: NEUTRAL_WHEEL,
      blending: 50,
      balance: 0,
    }
  );
}

const wheelSet = (w: GradeWheel) => w.saturation !== 0 || w.luminance !== 0;

/** Whether any wheel is set (Blending and Balance alone change nothing). */
export function gradingEdited(r: EditRecipe): boolean {
  const g = r.colourGrading;
  return g !== undefined && GRADE_RANGES.some((range) => wheelSet(g[range.id]));
}

/** `r` with its grading changed; left out while no wheel is set, as the renderer
 *  writes it. */
export function withGrading(r: EditRecipe, change: Partial<ColourGrading>): EditRecipe {
  const g = { ...gradingOf(r), ...change };
  const { colourGrading: _, ...rest } = r;
  return GRADE_RANGES.some((range) => wheelSet(g[range.id])) ? { ...rest, colourGrading: g } : rest;
}

/** `r` with one range's wheel changed. */
export function withWheel(r: EditRecipe, range: GradeRange, change: Partial<GradeWheel>): EditRecipe {
  return withGrading(r, { [range]: { ...gradingOf(r)[range], ...change } });
}

/** A point on the wheel (-1..1 from the centre, y up) for a hue (degrees, 0 red,
 *  counter-clockwise) and saturation (0..100). */
export function wheelPoint(w: GradeWheel): { x: number; y: number } {
  const t = (w.hue * Math.PI) / 180;
  const r = w.saturation / 100;
  return { x: r * Math.cos(t), y: r * Math.sin(t) };
}

/** The wheel's point moved by (dx, dy) (the wheel's radius is 1, y up), as the
 *  arrow keys move it. A move of at least 0.02 always changes the rounded result. */
export function nudgeWheel(w: GradeWheel, dx: number, dy: number): { hue: number; saturation: number } {
  const p = wheelPoint(w);
  return wheelAt(p.x + dx, p.y + dy);
}

/** The hue and saturation of a point on the wheel; outside it counts as its edge. */
export function wheelAt(x: number, y: number): { hue: number; saturation: number } {
  const r = Math.min(1, Math.hypot(x, y));
  const hue = ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
  return { hue: Math.round(hue), saturation: Math.round(r * 100) };
}
