import { useMemo, useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import type { FrameHistogram } from "../../ipc/frame";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { histogramPaths } from "./histogramGraph";
import {
  DIAGONAL,
  curvePath,
  insertPoint,
  isDiagonal,
  movePoint,
  nearestPoint,
  removePoint,
  resetEnd,
  toneValue,
  type CurvePoint,
} from "./pointCurve";

/** Grid and size from the design (a 288 x 120 viewBox, quarters marked). */
const W = 288;
const H = 120;
/** How close (px) a press must be to grab a point. */
const GRAB_PX = 9;
/** How far (px) outside the graph a dragged inner point goes to be removed. */
const REMOVE_PX = 24;

interface Drag {
  index: number;
  start: CurvePoint;
  x: number;
  y: number;
  points: CurvePoint[];
}

/**
 * The Light section's tone curve (ADR 0037), as in Lightroom's point curve: press
 * anywhere to add a point on the curve and drag it; drag a point to move it; drag an
 * inner point off the graph, press Delete, or double-click it to remove it. The end
 * points set the black and white levels. The photo's histogram sits behind the curve.
 */
export function ToneCurve({
  recipe,
  onChange,
  disabled,
  histogram,
}: {
  recipe: EditRecipe;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
  histogram: FrameHistogram | null;
}) {
  const points: CurvePoint[] = recipe.pointCurve ?? [...DIAGONAL];
  const boxRef = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  // While an inner point is dragged off the graph, it is shown removed.
  const [removing, setRemoving] = useState(false);
  const [active, setActive] = useState<number | null>(null);

  const shown = removing && active !== null ? removePoint(points, active) : points;
  const path = useMemo(() => curvePath(shown, W, H), [shown]);
  const backdrop = useMemo(() => (histogram ? histogramPaths(histogram).luma : null), [histogram]);
  const edited = !isDiagonal(points);

  const commit = (next: CurvePoint[]) =>
    onChange({ ...recipe, pointCurve: isDiagonal(next) ? undefined : next });

  const frac = (e: { clientX: number; clientY: number }) => {
    const box = boxRef.current!.getBoundingClientRect();
    return {
      x: (e.clientX - box.left) / box.width,
      y: 1 - (e.clientY - box.top) / box.height,
      box,
    };
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (disabled || e.button !== 0) return;
    const { x, y, box } = frac(e);
    let index = nearestPoint(points, x, y, GRAB_PX / box.width, GRAB_PX / box.height);
    let next = points;
    if (index === null) {
      const added = insertPoint(points, x);
      if (!added) return;
      ({ index, points: next } = added);
      commit(next);
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { index, start: next[index]!, x: e.clientX, y: e.clientY, points: next };
    setActive(index);
    e.preventDefault();
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const box = boxRef.current!.getBoundingClientRect();
    const inner = d.index > 0 && d.index < d.points.length - 1;
    const outside =
      e.clientY < box.top - REMOVE_PX ||
      e.clientY > box.bottom + REMOVE_PX ||
      e.clientX < box.left - REMOVE_PX ||
      e.clientX > box.right + REMOVE_PX;
    setRemoving(inner && outside);
    if (inner && outside) return;
    const moved = movePoint(
      d.points,
      d.index,
      d.start[0] + (e.clientX - d.x) / box.width,
      d.start[1] - (e.clientY - d.y) / box.height,
    );
    commit(moved);
  };

  const endDrag = () => {
    const d = drag.current;
    if (d && removing) {
      commit(removePoint(points, d.index));
      setActive(null);
    }
    drag.current = null;
    setRemoving(false);
    setActive(null);
  };

  const onDoubleClick = (i: number) => {
    if (disabled) return;
    commit(i === 0 || i === points.length - 1 ? resetEnd(points, i) : removePoint(points, i));
    setActive(null);
  };

  const onKeyDown = (i: number) => (e: KeyboardEvent<HTMLSpanElement>) => {
    if (disabled) return;
    const step = (e.shiftKey ? 10 : 1) / 255;
    const [x, y] = points[i]!;
    const moves: Record<string, CurvePoint> = {
      ArrowUp: [x, y + step],
      ArrowDown: [x, y - step],
      ArrowRight: [x + step, y],
      ArrowLeft: [x - step, y],
    };
    const to = moves[e.key];
    if (to) {
      commit(movePoint(points, i, to[0], to[1]));
    } else if (e.key === "Delete" || e.key === "Backspace") {
      commit(removePoint(points, i));
    } else {
      return;
    }
    e.preventDefault();
  };

  const readout = active !== null && !removing ? points[active] : undefined;

  return (
    <>
      <div className="tone-curve-title">
        <span>Tone curve</span>
        {readout ? (
          <span className="tone-curve-readout">
            In {toneValue(readout[0])} · Out {toneValue(readout[1])}
          </span>
        ) : (
          edited && (
            <button className="tone-curve-reset" disabled={disabled} onClick={() => commit([...DIAGONAL])}>
              Reset
            </button>
          )
        )}
      </div>
      <div
        ref={boxRef}
        className={`tone-curve${disabled ? "" : " editable"}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
      >
        {backdrop && (
          <svg viewBox="0 0 288 72" preserveAspectRatio="none" aria-hidden="true">
            <path className="backdrop" d={`${backdrop} L288 72 L0 72 Z`} />
          </svg>
        )}
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label="Tone curve">
          {[72, 144, 216].map((x) => (
            <line key={`x${x}`} className="grid" x1={x} y1={0} x2={x} y2={H} />
          ))}
          {[30, 60, 90].map((y) => (
            <line key={`y${y}`} className="grid" x1={0} y1={y} x2={W} y2={y} />
          ))}
          <line className="identity" x1={0} y1={H} x2={W} y2={0} />
          <path className="curve" d={path} />
        </svg>
        {shown.map(([x, y], i) => (
          <span
            key={i}
            className={`curve-point${i === active ? " active" : ""}`}
            style={{ left: `${x * 100}%`, top: `${(1 - y) * 100}%` }}
            role="slider"
            tabIndex={disabled ? -1 : 0}
            aria-label={i === 0 ? "Black point" : i === shown.length - 1 ? "White point" : `Curve point ${i}`}
            aria-valuemin={0}
            aria-valuemax={255}
            aria-valuenow={toneValue(y)}
            aria-valuetext={`Input ${toneValue(x)}, output ${toneValue(y)}`}
            onFocus={() => setActive(i)}
            onBlur={() => setActive((a) => (a === i && !drag.current ? null : a))}
            onKeyDown={onKeyDown(i)}
            onDoubleClick={() => onDoubleClick(i)}
          />
        ))}
      </div>
    </>
  );
}
