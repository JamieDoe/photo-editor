import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode, type RefObject } from "react";
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
  // Pointer moves only move the rings: the strokes are drawn again only when they
  // (or the picture's size) change.
  const { content, defs } = useMemo(
    () => strokeLayers(shape.strokes, prefix, space),
    [shape.strokes, prefix, W, H, D, space.crop.x, space.crop.y, space.crop.w, space.crop.h],
  );
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
      </svg>
      {/* The rings in a layer of their own, so moving them repaints nothing else. */}
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

/** Rings a stroke's soft edge is drawn with. */
const EDGE_STEPS = 10;

/**
 * The widths and opacities of nested round-capped paths whose stacked opacity is the
 * renderer's brush profile (ADR 0042): full within the unfeathered radius, smoothstep
 * to nothing at the radius. A path of half-width `d` covers every point within `d` of
 * the stroke, so ring k (from the outside in) adds its opacity inside distance `d`;
 * the opacities are chosen so the total over each ring's band is the profile at the
 * band's middle. Exported for tests.
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

/**
 * The strokes as SVG, composed as the renderer does: each stroke its profile at its
 * flow, paint drawn over what is there, and each erase stroke masking out everything
 * painted before it. Paint is `currentColor`, so the same layers draw the tint or an
 * inverted mask. No filters: they are slow in the app's web view.
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
    const rings = edgeRings(s.size * D, s.feather);
    // The stroke at its flow: its rings stacked in a group, the group at the flow.
    const shape = (colour: string) => (
      <g opacity={s.flow / 100}>
        {rings.map((ring, k) => (
          <path
            key={k}
            d={path}
            fill="none"
            stroke={colour}
            strokeWidth={2 * ring.halfWidth}
            strokeOpacity={ring.opacity}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ))}
      </g>
    );
    if (!s.erase) {
      content.push(<g key={i}>{shape("currentColor")}</g>);
      return;
    }
    const id = `${prefix}-erase-${i}`;
    defs.push(
      <mask key={id} id={id} maskUnits="userSpaceOnUse" x={0} y={0} width={W} height={H}>
        <rect width={W} height={H} fill="white" />
        {shape("black")}
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
