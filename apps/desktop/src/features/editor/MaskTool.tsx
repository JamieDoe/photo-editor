import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { EyeIcon } from "../../components/icons";
import type { Combine } from "../../ipc/generated/Combine";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { Mask } from "../../ipc/generated/Mask";
import type { MaskShape } from "../../ipc/generated/MaskShape";
import type { Stroke } from "../../ipc/generated/Stroke";
import { BrushGuides } from "./BrushOverlay";
import { MaskTintCanvas } from "./MaskTintCanvas";
import { isTextEntry } from "../../lib/keyboard";
import {
  ADDABLE_KINDS,
  DEFAULT_BRUSH,
  FULL_CROP,
  MASK_KINDS,
  addShape,
  maskName,
  masksOf,
  newMask,
  newShape,
  normaliseAngle,
  removeShape,
  setAdjustment,
  setShapeMode,
  shapesOf,
  toShown,
  updateMask,
  withMasks,
  withShapeAt,
  type BrushSettings,
  type MaskKind,
  type Point,
} from "./masks";

type Radial = Extract<MaskShape, { kind: "radial" }>;
type Linear = Extract<MaskShape, { kind: "linear" }>;

/**
 * Mask mode (ADRs 0040–0043): which mask is being edited and which of its shapes,
 * whether its coverage is shown, and the edits the toolbar, the overlay and the
 * Selective section make. The masks themselves live in the recipe.
 */
export function useMaskTool(opts: {
  recipe: EditRecipe | null;
  imageId: number | null;
  onChange: (r: EditRecipe) => void;
}) {
  const { recipe, imageId, onChange } = opts;
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [shapeAt, setShapeAt] = useState(0);
  const [overlay, setOverlay] = useState(true);
  const [brush, setBrushSettings] = useState<BrushSettings>(DEFAULT_BRUSH);
  const masks = recipe ? masksOf(recipe) : [];
  const active = masks.find((m) => m.id === activeId) ?? null;
  // The shape being edited: the one picked, or the last if it was removed.
  const shapes = active ? shapesOf(active) : [];
  const shapeIndex = Math.max(0, Math.min(shapeAt, shapes.length - 1));
  const shape = shapes[shapeIndex]?.shape ?? null;
  const crop: CropRect = recipe?.geometry?.crop ?? FULL_CROP;

  // Another photo: leave mask mode.
  useEffect(() => {
    setOpen(false);
    setActiveId(null);
  }, [imageId]);
  // The active mask removed (or an older edit restored): pick the first.
  useEffect(() => {
    if (activeId !== null && !masks.some((m) => m.id === activeId)) setActiveId(masks[0]?.id ?? null);
  }, [masks, activeId]);

  const commit = useCallback(
    (next: Mask[]) => {
      if (recipe) onChange(withMasks(recipe, next));
    },
    [recipe, onChange],
  );
  const changeActive = (change: (m: Mask) => Mask) => {
    if (active) commit(updateMask(masks, active.id, change));
  };
  /** Changes the shape being edited. */
  const changeShape = (change: (s: MaskShape) => MaskShape) => {
    if (shape) changeActive((m) => withShapeAt(m, shapeIndex, change(shape)));
  };
  const pickMask = (id: number) => {
    if (id !== activeId) setShapeAt(0);
    setActiveId(id);
  };

  return {
    open,
    masks,
    active,
    overlay,
    crop,
    enter: () => {
      setOpen(true);
      if (activeId === null) setActiveId(masks[0]?.id ?? null);
    },
    done: () => setOpen(false),
    pick: (id: number) => {
      pickMask(id);
      setOpen(true);
    },
    add: (kind: MaskKind) => {
      const m = newMask(kind, masks, crop);
      commit([...masks, m]);
      pickMask(m.id);
      setOpen(true);
    },
    remove: (id: number) => commit(masks.filter((m) => m.id !== id)),
    /** The active mask's shapes (ADR 0043), and which is being edited. */
    shapes,
    shapeIndex,
    shape,
    pickShape: (i: number) => {
      setShapeAt(i);
      setOpen(true);
    },
    /** Another shape of `kind` in the active mask, combined as `mode`; edited next. */
    addShape: (mode: Combine, kind: MaskKind) => {
      if (!active) return;
      changeActive((m) => addShape(m, mode, newShape(kind, crop)));
      setShapeAt(shapes.length);
      setOpen(true);
    },
    removeShape: (i: number) => {
      changeActive((m) => removeShape(m, i));
      if (i < shapeIndex) setShapeAt(shapeIndex - 1);
    },
    setShapeMode: (i: number, mode: Combine) => changeActive((m) => setShapeMode(m, i, mode)),
    /** How strongly the active mask applies, 0..100. */
    setDensity: (density: number) => changeActive((m) => ({ ...m, density: density === 100 ? undefined : density })),
    /** Hides a mask's effect, or shows it again; it stays in the list either way. */
    toggleHidden: (id: number) => commit(updateMask(masks, id, (m) => ({ ...m, hidden: m.hidden ? undefined : true }))),
    /** Replaces the shape being edited. */
    setShape: (next: MaskShape) => changeShape(() => next),
    setAdjustment: (key: keyof LocalAdjustments, value: number) => {
      if (active) commit(setAdjustment(masks, active.id, key, value));
    },
    /** Adjust outside the shape instead (ADR 0041). */
    setInvert: (invert: boolean) => changeActive((m) => ({ ...m, invert: invert || undefined })),
    /** A radial shape's Feather. */
    setFeather: (feather: number) => changeShape((s) => (s.kind === "radial" ? { ...s, feather } : s)),
    toggleOverlay: () => setOverlay((o) => !o),
    /** The brush the next stroke is painted with (ADR 0042). */
    brush,
    setBrush: (change: Partial<BrushSettings>) => setBrushSettings((b) => ({ ...b, ...change })),
    /** The brush shape's strokes (while painting, the last is in progress). */
    setStrokes: (strokes: Stroke[]) => changeShape((s) => (s.kind === "brush" ? { kind: "brush", strokes } : s)),
  };
}

export type MaskTool = ReturnType<typeof useMaskTool>;

/** Where the overlay draws: the shown picture's pixels, and the frame's diagonal in
 *  them (radial radii are fractions of it). */
interface Space {
  w: number;
  h: number;
  diagonal: number;
  crop: CropRect;
}

/**
 * The active mask on the photo, as in the design: its coverage tinted in the accent
 * colour (when the overlay is on), and the guides and handles of the shape being
 * edited.
 */
export function MaskOverlay({ tool, size }: { tool: MaskTool; size: { width: number; height: number } }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const mask = tool.active;
  const shape = tool.shape;
  if (!mask || !shape) return <div className="mask-overlay" />;
  const crop = tool.crop;
  const space: Space = {
    w: size.width,
    h: size.height,
    diagonal: Math.hypot(size.width / crop.w, size.height / crop.h),
    crop,
  };
  const props = { tool, space, boxRef };
  // Another shape starts its guides afresh.
  const key = `${mask.id}:${tool.shapeIndex}`;
  return (
    <div className="mask-overlay" ref={boxRef}>
      {tool.overlay && !mask.hidden && <MaskTintCanvas mask={mask} crop={crop} boxRef={boxRef} />}
      {shape.kind === "linear" ? (
        <LinearGuides key={key} {...props} shape={shape} />
      ) : shape.kind === "radial" ? (
        <RadialGuides key={key} {...props} shape={shape} />
      ) : (
        <BrushGuides key={key} {...props} shape={shape} />
      )}
    </div>
  );
}

interface GuideProps<S> {
  tool: MaskTool;
  space: Space;
  boxRef: RefObject<HTMLDivElement | null>;
  shape: S;
}

/** Dragging a handle: the pointer in the shown picture's pixels, and in frame
 *  fractions since the press. */
function useHandleDrag(boxRef: RefObject<HTMLDivElement | null>, space: Space) {
  const drag = useRef<{ x: number; y: number; onMove: (p: Point, delta: Point) => void } | null>(null);
  const begin = (onMove: (p: Point, delta: Point) => void) => (ev: ReactPointerEvent) => {
    ev.preventDefault();
    ev.stopPropagation();
    (ev.target as Element).setPointerCapture(ev.pointerId);
    drag.current = { x: ev.clientX, y: ev.clientY, onMove };
  };
  const move = (ev: ReactPointerEvent) => {
    const d = drag.current;
    const box = boxRef.current?.getBoundingClientRect();
    if (!d || !box) return;
    const p: Point = [((ev.clientX - box.left) / box.width) * space.w, ((ev.clientY - box.top) / box.height) * space.h];
    const delta: Point = [((ev.clientX - d.x) / box.width) * space.crop.w, ((ev.clientY - d.y) / box.height) * space.crop.h];
    d.onMove(p, delta);
  };
  const end = () => {
    drag.current = null;
  };
  return { begin, handlers: { onPointerMove: move, onPointerUp: end, onPointerCancel: end } };
}

const pct = (v: number) => `${(v * 100).toFixed(3)}%`;

function HandleDot({
  kind,
  at,
  space,
  label,
  onPointerDown,
  handlers,
}: {
  kind: string;
  at: Point;
  space: Space;
  label: string;
  onPointerDown: (ev: ReactPointerEvent) => void;
  handlers: Record<string, (ev: ReactPointerEvent) => void>;
}) {
  return (
    <span
      className={`mask-handle ${kind}`}
      style={{ left: pct(at[0] / space.w), top: pct(at[1] / space.h) }}
      title={label}
      aria-label={label}
      onPointerDown={onPointerDown}
      {...handlers}
    />
  );
}

/** A linear gradient: the dashed start and end lines and the solid centre line. Drag
 *  the centre to move it, the start or end points to turn it or change its fade. */
function LinearGuides({ tool, space, boxRef, shape }: GuideProps<Linear>) {
  const { begin, handlers } = useHandleDrag(boxRef, space);
  const { w: W, h: H } = space;
  const toPx = (p: Point): Point => {
    const s = toShown(p, space.crop);
    return [s[0] * W, s[1] * H];
  };
  const s = toPx(shape.start);
  const e = toPx(shape.end);
  const c: Point = [(s[0] + e[0]) / 2, (s[1] + e[1]) / 2];
  const len = Math.hypot(e[0] - s[0], e[1] - s[1]) || 1;
  const n = [(-(e[1] - s[1]) / len) * (W + H), ((e[0] - s[0]) / len) * (W + H)];
  const line = (p: Point) => ({ x1: p[0] - n[0]!, y1: p[1] - n[1]!, x2: p[0] + n[0]!, y2: p[1] + n[1]! });
  const shift = (p: Point, d: Point): Point => [p[0] + d[0], p[1] + d[1]];
  const moveBy = (which: "start" | "end" | "both") =>
    begin((_, d) =>
      tool.setShape({
        kind: "linear",
        start: which === "end" ? shape.start : shift(shape.start, d),
        end: which === "start" ? shape.end : shift(shape.end, d),
      }),
    );
  return (
    <>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
        <line className="mask-line dashed" {...line(s)} />
        <line className="mask-line" {...line(c)} />
        <line className="mask-line dashed" {...line(e)} />
      </svg>
      <HandleDot kind="start" at={s} space={space} label="Full effect from here" onPointerDown={moveBy("start")} handlers={handlers} />
      <HandleDot kind="end" at={e} space={space} label="No effect from here" onPointerDown={moveBy("end")} handlers={handlers} />
      <HandleDot kind="centre" at={c} space={space} label="Move" onPointerDown={moveBy("both")} handlers={handlers} />
    </>
  );
}

/** A radial gradient: its dashed ellipse and, fainter, where the fade starts. Drag the
 *  centre to move it, a side handle to stretch and turn it, the top or bottom handle
 *  for its other radius. */
function RadialGuides({ tool, space, boxRef, shape }: GuideProps<Radial>) {
  const { begin, handlers } = useHandleDrag(boxRef, space);
  const { w: W, h: H, diagonal } = space;
  const shown = toShown(shape.centre, space.crop);
  const c: Point = [shown[0] * W, shown[1] * H];
  const [a, b] = [shape.radius[0] * diagonal, shape.radius[1] * diagonal];
  const t = (shape.angle * Math.PI) / 180;
  const [cos, sin] = [Math.cos(t), Math.sin(t)];
  const along = (k: number): Point => [c[0] + k * a * cos, c[1] + k * a * sin];
  const across = (k: number): Point => [c[0] - k * b * sin, c[1] + k * b * cos];
  const inner = 1 - shape.feather / 100;
  const set = (change: Partial<Radial>) => tool.setShape({ ...shape, ...change });
  const round = (v: number) => Math.round(v * 1e5) / 1e5;
  const side = (k: 1 | -1) =>
    begin((p) => {
      const v: Point = [(p[0] - c[0]) * k, (p[1] - c[1]) * k];
      set({
        radius: [round(Math.max(Math.hypot(v[0], v[1]), 2) / diagonal), shape.radius[1]],
        angle: normaliseAngle(Math.round((Math.atan2(v[1], v[0]) * 180) / Math.PI * 10) / 10),
      });
    });
  const top = () =>
    begin((p) => {
      const reach = Math.abs(-(p[0] - c[0]) * sin + (p[1] - c[1]) * cos);
      set({ radius: [shape.radius[0], round(Math.max(reach, 2) / diagonal)] });
    });
  const centre = begin((_, d) => set({ centre: [shape.centre[0] + d[0], shape.centre[1] + d[1]] }));
  const ellipse = (k: number) => ({
    cx: c[0],
    cy: c[1],
    rx: Math.max(a * k, 0.01),
    ry: Math.max(b * k, 0.01),
    transform: `rotate(${shape.angle} ${c[0]} ${c[1]})`,
  });
  return (
    <>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
        {inner > 0.02 && <ellipse className="mask-line faint" {...ellipse(inner)} />}
        <ellipse className="mask-line dashed" {...ellipse(1)} />
      </svg>
      <HandleDot kind="end" at={along(1)} space={space} label="Stretch and turn" onPointerDown={side(1)} handlers={handlers} />
      <HandleDot kind="end" at={along(-1)} space={space} label="Stretch and turn" onPointerDown={side(-1)} handlers={handlers} />
      <HandleDot kind="end" at={across(1)} space={space} label="Stretch" onPointerDown={top()} handlers={handlers} />
      <HandleDot kind="end" at={across(-1)} space={space} label="Stretch" onPointerDown={top()} handlers={handlers} />
      <HandleDot kind="centre" at={c} space={space} label="Move" onPointerDown={centre} handlers={handlers} />
    </>
  );
}

/** The floating toolbar in mask mode, as in the design: the masks, Add, the overlay
 *  switch and Done. */
export function MaskToolbar({ tool }: { tool: MaskTool }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTextEntry(e.target) || e.target instanceof HTMLInputElement) return;
      if (e.key === "Enter" || e.key === "Escape") tool.done();
      // [ and ] resize the brush, as in other editors.
      if ((e.key === "[" || e.key === "]") && tool.shape?.kind === "brush") {
        const size = e.key === "[" ? tool.brush.size / 1.15 : tool.brush.size * 1.15;
        tool.setBrush({ size: Math.min(0.25, Math.max(0.0025, size)) });
        e.preventDefault();
      }
      // Delete removes the shape being edited, or the mask when it is its only one.
      if ((e.key === "Delete" || e.key === "Backspace") && tool.active) {
        if (tool.shapes.length > 1) tool.removeShape(tool.shapeIndex);
        else tool.remove(tool.active.id);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tool]);
  return (
    <div className="photo-toolbar mask-toolbar" role="toolbar" aria-label="Masks">
      {tool.masks.map((m) => (
        <button
          key={m.id}
          className={m.hidden ? "mask-chip hidden" : "mask-chip"}
          aria-pressed={tool.active?.id === m.id}
          title={m.hidden ? "Hidden" : undefined}
          onClick={() => tool.pick(m.id)}
        >
          <span className="mask-dot" style={{ background: MASK_KINDS[m.shape.kind].dot }} />
          {maskName(tool.masks, m)}
        </button>
      ))}
      {tool.masks.length > 0 && <span className="toolbar-divider" />}
      <span className="mask-add-label">Add</span>
      {ADDABLE_KINDS.map((k) => (
        <button key={k} className="mask-add" title={MASK_KINDS[k].hint} onClick={() => tool.add(k)}>
          {MASK_KINDS[k].add}
        </button>
      ))}
      <span className="toolbar-divider" />
      <button
        className="crop-icon"
        aria-pressed={tool.overlay}
        aria-label="Show mask overlay"
        title="Show overlay"
        onClick={tool.toggleOverlay}
      >
        <EyeIcon size={15} />
      </button>
      <button className="primary crop-done" onClick={tool.done}>
        Done
      </button>
    </div>
  );
}
