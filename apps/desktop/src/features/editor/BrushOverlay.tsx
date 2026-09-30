import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from "react";
import type { Mask } from "../../ipc/generated/Mask";
import type { MaskShape } from "../../ipc/generated/MaskShape";
import type { Stroke } from "../../ipc/generated/Stroke";
import type { MaskTool } from "./MaskTool";
import { fromShown, toShown, type Point } from "./masks";

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
 * brush's size and, dashed, where its soft edge starts. The tint is drawn from the
 * strokes themselves, in order, as the renderer composes them.
 */
export function BrushGuides({
  tool,
  mask,
  space,
  boxRef,
  shape,
}: {
  tool: MaskTool;
  mask: Mask;
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
  const prefix = `brush-${mask.id}`;
  const { content, defs } = strokeLayers(shape.strokes, prefix, space);
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
        <defs>
          {defs}
          {mask.invert && (
            <mask id={`${prefix}-invert`} maskUnits="userSpaceOnUse" x={0} y={0} width={W} height={H}>
              <rect width={W} height={H} fill="white" />
              <g style={{ color: "black" }}>{content}</g>
            </mask>
          )}
        </defs>
        {tool.overlay &&
          !mask.hidden &&
          (mask.invert ? (
            <rect className="brush-tint" width={W} height={H} mask={`url(#${prefix}-invert)`} />
          ) : (
            <g className="brush-tint">{content}</g>
          ))}
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

/**
 * The strokes as SVG, composed as the renderer does: paint strokes drawn over each
 * other at their flow, each erase stroke masking out everything painted before it.
 * Paint is `currentColor`, so the same layers draw the tint or an inverted mask.
 */
function strokeLayers(strokes: Stroke[], prefix: string, space: Space): { content: ReactNode; defs: ReactNode[] } {
  const { w: W, h: H, diagonal: D } = space;
  const defs: ReactNode[] = [];
  let content: ReactNode[] = [];
  strokes.forEach((s, i) => {
    if (s.points.length === 0) return;
    const d = s.points
      .map((p, k) => {
        const q = toShown(p, space.crop);
        return `${k ? "L" : "M"}${(q[0] * W).toFixed(1)} ${(q[1] * H).toFixed(1)}`;
      })
      .join(" ");
    // A single point is a zero-length path: its round caps make the dab.
    const path = s.points.length === 1 ? `${d} l0.01 0` : d;
    const width = 2 * s.size * D;
    const blur = (s.size * D * s.feather) / 100 / 2.5;
    const filter = blur > 0.5 ? `${prefix}-blur-${i}` : null;
    if (filter) {
      defs.push(
        <filter key={filter} id={filter} filterUnits="userSpaceOnUse" x={0} y={0} width={W} height={H}>
          <feGaussianBlur stdDeviation={blur} />
        </filter>,
      );
    }
    const line = (colour: string) => (
      <path
        d={path}
        fill="none"
        stroke={colour}
        strokeWidth={width}
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity={s.flow / 100}
        filter={filter ? `url(#${filter})` : undefined}
      />
    );
    if (!s.erase) {
      content.push(<g key={i}>{line("currentColor")}</g>);
      return;
    }
    const id = `${prefix}-erase-${i}`;
    defs.push(
      <mask key={id} id={id} maskUnits="userSpaceOnUse" x={0} y={0} width={W} height={H}>
        <rect width={W} height={H} fill="white" />
        {line("black")}
      </mask>,
    );
    content = [
      <g key={i} mask={`url(#${id})`}>
        {content}
      </g>,
    ];
  });
  return { content, defs };
}
