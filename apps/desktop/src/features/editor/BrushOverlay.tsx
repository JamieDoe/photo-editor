import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import type { MaskShape } from "../../ipc/generated/MaskShape";
import type { Stroke } from "../../ipc/generated/Stroke";
import type { MaskTool } from "./MaskTool";
import { fromShown, type Point } from "./masks";

type Brush = Extract<MaskShape, { kind: "brush" }>;

/** The shown picture's pixels and the frame's diagonal in them (brush sizes are
 *  fractions of it). */
interface Space {
  w: number;
  h: number;
  diagonal: number;
  crop: { x: number; y: number; w: number; h: number };
}

/** New points closer than this (screen pixels) to the last are skipped. */
const MIN_STEP_PX = 2;
const round = (v: number) => Math.round(v * 10_000) / 10_000;

/**
 * A brush mask on the photo (ADR 0042): paint by dragging (holding Option, or with
 * Erase chosen, takes paint away). The design's two rings follow the pointer: the
 * brush's size and, dashed, where its soft edge starts. The tint is the mask's (see
 * `MaskTint`).
 */
export function BrushGuides({
  tool,
  space,
  boxRef,
  shape,
}: {
  tool: MaskTool;
  space: Space;
  boxRef: RefObject<HTMLDivElement | null>;
  shape: Brush;
}) {
  const { w: W, h: H, diagonal: D } = space;
  const [pointer, setPointer] = useState<Point | null>(null);
  const [alt, setAlt] = useState(false);
  const drawing = useRef<{ base: Stroke[]; stroke: Stroke; last: [number, number]; frame: number | null } | null>(null);
  useEffect(
    () => () => {
      const d = drawing.current;
      if (d?.frame != null) cancelAnimationFrame(d.frame);
    },
    [],
  );

  const at = (ev: ReactPointerEvent) => {
    const box = boxRef.current!.getBoundingClientRect();
    const u = (ev.clientX - box.left) / box.width;
    const v = (ev.clientY - box.top) / box.height;
    return { shown: [u * W, v * H] as Point, frame: fromShown([u, v], space.crop) };
  };
  const commit = () => {
    const d = drawing.current;
    if (!d) return;
    d.frame = null;
    tool.setStrokes([...d.base, { ...d.stroke, points: [...d.stroke.points] }]);
  };

  const onPointerDown = (ev: ReactPointerEvent<HTMLDivElement>) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.currentTarget.setPointerCapture(ev.pointerId);
    const { frame } = at(ev);
    const b = tool.brush;
    drawing.current = {
      base: shape.strokes,
      stroke: {
        erase: b.erase || ev.altKey ? true : undefined,
        size: b.size,
        feather: b.feather,
        flow: b.flow,
        points: [[round(frame[0]), round(frame[1])]],
      },
      last: [ev.clientX, ev.clientY],
      frame: null,
    };
    commit();
  };
  const onPointerMove = (ev: ReactPointerEvent<HTMLDivElement>) => {
    setPointer(at(ev).shown);
    setAlt(ev.altKey);
    const d = drawing.current;
    if (!d || Math.hypot(ev.clientX - d.last[0], ev.clientY - d.last[1]) < MIN_STEP_PX) return;
    const { frame } = at(ev);
    d.stroke.points.push([round(frame[0]), round(frame[1])]);
    d.last = [ev.clientX, ev.clientY];
    // At most one edit per display frame.
    if (d.frame == null) d.frame = requestAnimationFrame(commit);
  };
  const onPointerUp = () => {
    const d = drawing.current;
    if (!d) return;
    if (d.frame != null) cancelAnimationFrame(d.frame);
    commit();
    drawing.current = null;
  };

  const erasing = tool.brush.erase || alt;
  const r = tool.brush.size * D;
  const inner = r * (1 - tool.brush.feather / 100);
  return (
    <div
      className="brush-surface"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onPointerLeave={() => setPointer(null)}
    >
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
        {pointer && (
          <g className={erasing ? "brush-cursor erase" : "brush-cursor"}>
            <circle cx={pointer[0]} cy={pointer[1]} r={r} className="brush-ring" />
            {inner > 1 && inner < r - 1 && <circle cx={pointer[0]} cy={pointer[1]} r={inner} className="brush-ring inner" />}
            {erasing && (
              <line
                x1={pointer[0] - Math.min(r, 12) / 2}
                y1={pointer[1]}
                x2={pointer[0] + Math.min(r, 12) / 2}
                y2={pointer[1]}
                className="brush-ring"
              />
            )}
          </g>
        )}
      </svg>
    </div>
  );
}
