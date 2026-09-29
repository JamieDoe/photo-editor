import { useEffect, useState } from "react";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import * as ipc from "../../ipc/client";

/** The recipe fields the tone curve depends on. */
export function toneCurveKey(r: EditRecipe): string {
  return [r.exposure, r.contrast, r.highlights, r.shadows, r.whites, r.blacks, r.look].join("|");
}

/**
 * The tone curve for `recipe`, fetched from the renderer when a field it depends on
 * changes. Replies that arrive after a newer request are ignored.
 */
export function useToneCurve(recipe: EditRecipe | null): number[] | null {
  const [points, setPoints] = useState<number[] | null>(null);
  const key = recipe ? toneCurveKey(recipe) : null;
  useEffect(() => {
    if (!recipe) return;
    let current = true;
    ipc
      .toneCurve(recipe)
      .then((p) => {
        if (current) setPoints(p);
      })
      .catch(() => {
        // The graph is informational; keep the last curve.
      });
    return () => {
      current = false;
    };
    // Only the fields the curve depends on (see toneCurveKey).
  }, [key]);
  return points;
}
