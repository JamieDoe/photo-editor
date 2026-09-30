import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { EyeIcon } from "../../components/icons";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { Mask } from "../../ipc/generated/Mask";
import type { MaskShape } from "../../ipc/generated/MaskShape";
import { isTextEntry } from "../../lib/keyboard";
import {
  FULL_CROP,
  MASK_KINDS,
  maskName,
  masksOf,
  newLinearMask,
  setAdjustment,
  toShown,
  updateMask,
  withMasks,
  type MaskKind,
  type Point,
} from "./masks";

/**
 * Mask mode (ADR 0040): which mask is being edited, whether its coverage is shown,
 * and the edits the toolbar, the overlay and the Selective section make. The masks
 * themselves live in the recipe.
 */
export function useMaskTool(opts: {
  recipe: EditRecipe | null;
  imageId: number | null;
  onChange: (r: EditRecipe) => void;
}) {
  const { recipe, imageId, onChange } = opts;
  const [open, setOpen] = useState(false);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [overlay, setOverlay] = useState(true);
  const masks = recipe ? masksOf(recipe) : [];
  const active = masks.find((m) => m.id === activeId) ?? null;
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
      setActiveId(id);
      setOpen(true);
    },
    add: (kind: MaskKind) => {
      const m = kind === "linear" ? newLinearMask(masks, crop) : null;
      if (!m) return;
      commit([...masks, m]);
      setActiveId(m.id);
      setOpen(true);
    },
    remove: (id: number) => commit(masks.filter((m) => m.id !== id)),
    setShape: (id: number, shape: MaskShape) => commit(updateMask(masks, id, (m) => ({ ...m, shape }))),
    setAdjustment: (key: keyof LocalAdjustments, value: number) => {
      if (active) commit(setAdjustment(masks, active.id, key, value));
    },
    toggleOverlay: () => setOverlay((o) => !o),
  };
}

export type MaskTool = ReturnType<typeof useMaskTool>;

type Handle = "start" | "centre" | "end";

/**
 * The active mask on the photo, as in the design: its coverage tinted in the accent
 * colour (when the overlay is on) and a linear gradient's three lines, full effect at
 * the dashed start line, none at the dashed end line. Drag the centre to move it,
 * the start or end points to turn it or change how far it fades.
 */
export function MaskOverlay({ tool, size }: { tool: MaskTool; size: { width: number; height: number } }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ handle: Handle; x: number; y: number; start: Point; end: Point } | null>(null);
  const mask = tool.active;
  if (!mask) return <div className="mask-overlay" />;
  const { width: W, height: H } = size;
  const shape = mask.shape;
  const s = toShown(shape.start, tool.crop);
  const e = toShown(shape.end, tool.crop);
  const c: Point = [(s[0] + e[0]) / 2, (s[1] + e[1]) / 2];
  // Perpendicular lines in pixels, long enough to cross the picture.
  const d = [(e[0] - s[0]) * W, (e[1] - s[1]) * H];
  const len = Math.hypot(d[0]!, d[1]!) || 1;
  const n = [(-d[1]! / len) * (W + H), (d[0]! / len) * (W + H)];
  const line = (p: Point) => ({
    x1: p[0] * W - n[0]!,
    y1: p[1] * H - n[1]!,
    x2: p[0] * W + n[0]!,
    y2: p[1] * H + n[1]!,
  });
  const gradientId = `mask-coverage-${mask.id}`;
  // Coverage stops: 1 - smoothstep, as the renderer computes it.
  const stops = [0, 0.25, 0.5, 0.75, 1].map((t) => ({ t, a: 1 - t * t * (3 - 2 * t) }));

  const begin = (handle: Handle) => (ev: ReactPointerEvent) => {
    ev.preventDefault();
    ev.stopPropagation();
    (ev.target as Element).setPointerCapture(ev.pointerId);
    drag.current = { handle, x: ev.clientX, y: ev.clientY, start: shape.start, end: shape.end };
  };
  const move = (ev: ReactPointerEvent) => {
    const dg = drag.current;
    const box = boxRef.current?.getBoundingClientRect();
    if (!dg || !box) return;
    // Pointer movement in frame fractions.
    const dx = ((ev.clientX - dg.x) / box.width) * tool.crop.w;
    const dy = ((ev.clientY - dg.y) / box.height) * tool.crop.h;
    const shift = (p: Point): Point => [p[0] + dx, p[1] + dy];
    const next: MaskShape = {
      kind: "linear",
      start: dg.handle === "end" ? dg.start : shift(dg.start),
      end: dg.handle === "start" ? dg.end : shift(dg.end),
    };
    tool.setShape(mask.id, next);
  };
  const end = () => {
    drag.current = null;
  };
  const pct = (v: number) => `${(v * 100).toFixed(3)}%`;
  const handle = (h: Handle, p: Point, label: string) => (
    <span
      className={`mask-handle ${h}`}
      style={{ left: pct(p[0]), top: pct(p[1]) }}
      title={label}
      aria-label={label}
      onPointerDown={begin(h)}
    />
  );

  return (
    <div className="mask-overlay" ref={boxRef} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <linearGradient
            id={gradientId}
            gradientUnits="userSpaceOnUse"
            x1={s[0] * W}
            y1={s[1] * H}
            x2={e[0] * W}
            y2={e[1] * H}
          >
            {stops.map(({ t, a }) => (
              <stop key={t} offset={t} className="mask-tint-stop" stopOpacity={0.42 * a} />
            ))}
          </linearGradient>
        </defs>
        {tool.overlay && <rect width={W} height={H} fill={`url(#${gradientId})`} />}
        <line className="mask-line dashed" {...line(s)} />
        <line className="mask-line" {...line(c)} />
        <line className="mask-line dashed" {...line(e)} />
      </svg>
      {handle("start", s, "Full effect from here")}
      {handle("end", e, "No effect from here")}
      {handle("centre", c, "Move")}
    </div>
  );
}

/** The floating toolbar in mask mode, as in the design: the masks, Add, the overlay
 *  switch and Done. */
export function MaskToolbar({ tool }: { tool: MaskTool }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTextEntry(e.target) || e.target instanceof HTMLInputElement) return;
      if (e.key === "Enter" || e.key === "Escape") tool.done();
      if ((e.key === "Delete" || e.key === "Backspace") && tool.active) {
        tool.remove(tool.active.id);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tool]);
  return (
    <div className="photo-toolbar mask-toolbar" role="toolbar" aria-label="Masks">
      {tool.masks.map((m) => (
        <button key={m.id} className="mask-chip" aria-pressed={tool.active?.id === m.id} onClick={() => tool.pick(m.id)}>
          <span className="mask-dot" style={{ background: MASK_KINDS[m.shape.kind].dot }} />
          {maskName(tool.masks, m)}
        </button>
      ))}
      {tool.masks.length > 0 && <span className="toolbar-divider" />}
      <span className="mask-add-label">Add</span>
      <button className="mask-add" title="Graduated filter" onClick={() => tool.add("linear")}>
        Linear
      </button>
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
