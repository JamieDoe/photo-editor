import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Spot } from "../../ipc/generated/Spot";
import type { SpotKind } from "../../ipc/generated/SpotKind";
import { newSpot } from "../../ipc/client";
import { isTextEntry } from "../../lib/keyboard";
import { orientedSize } from "./cropGeometry";
import { FULL_CROP, fromShown, toShown, type Point } from "./masks";
import { Slider } from "./Slider";
import {
  BRUSH_SIZE,
  RETOUCH_TOOLS,
  frameToSource,
  moveSpot,
  placementOf,
  sourceToFrame,
  spotAt,
  spotsOf,
  withSpots,
} from "./spots";

/**
 * Retouch mode (ADR 0054): the tool for new spots (Heal or Clone) and the brush size,
 * the selected spot (which the panel's controls change instead), and the edits the
 * overlay and panel make. The spots themselves live in the recipe.
 */
export function useRetouchTool(opts: {
  recipe: EditRecipe | null;
  imageId: number | null;
  onChange: (r: EditRecipe) => void;
  notify: (message: string) => void;
}) {
  const { recipe, imageId, onChange, notify } = opts;
  const [kind, setKindState] = useState<SpotKind>("heal");
  const [size, setSizeState] = useState<number>(BRUSH_SIZE.initial);
  const [selected, setSelected] = useState<number | null>(null);
  // Screen pixels per unit of the photo's long edge, as the overlay last measured.
  const [scale, setScale] = useState(1000);
  const spots = recipe ? spotsOf(recipe) : [];
  const spot = selected !== null ? (spots[selected] ?? null) : null;
  // The latest recipe, for edits that finish after an await.
  const latest = useRef(recipe);
  latest.current = recipe;

  useEffect(() => setSelected(null), [imageId]);
  useEffect(() => {
    if (selected !== null && selected >= spots.length) setSelected(null);
  }, [selected, spots.length]);

  const commit = useCallback(
    (next: Spot[]) => {
      if (latest.current) onChange(withSpots(latest.current, next));
    },
    [onChange],
  );
  const update = (i: number, s: Spot) => commit(spots.map((old, j) => (j === i ? s : old)));
  const radiusFor = (px: number) => Math.round((px / 2 / scale) * 1e5) / 1e5;

  return {
    spots,
    selected,
    spot,
    kind: spot?.kind ?? kind,
    /** The brush size in screen pixels: the selected spot's, or the next one's. */
    size: spot ? Math.round(spot.radius * 2 * scale) : size,
    scale,
    setScale,
    select: setSelected,
    setKind: (k: SpotKind) => {
      setKindState(k);
      if (spot && selected !== null) update(selected, { ...spot, kind: k });
    },
    setSize: (px: number) => {
      setSizeState(px);
      if (spot && selected !== null) update(selected, { ...spot, radius: radiusFor(px) });
    },
    /** A new spot at source point `at`, its source found by the engine. */
    place: async (at: Point) => {
      if (imageId === null || !latest.current) return;
      const existing = spotsOf(latest.current);
      const made = await newSpot(imageId, kind, at, radiusFor(size), existing).catch(() => null);
      if (!made) {
        notify("No clean area nearby to take this spot from");
        return;
      }
      const now = latest.current ? spotsOf(latest.current) : existing;
      commit([...now, made]);
      setSelected(now.length);
    },
    update,
    remove: (i: number) => {
      commit(spots.filter((_, j) => j !== i));
      setSelected(null);
    },
    clear: () => {
      commit([]);
      setSelected(null);
    },
  };
}

export type RetouchTool = ReturnType<typeof useRetouchTool>;

/**
 * The spots on the photo: each a dashed circle in the design's amber, the selected
 * one white with its source joined to it. Click to add a spot; drag a spot or its
 * source to move it; Option-click to take the selected spot from there; Delete
 * removes it.
 */
export function RetouchOverlay({
  tool,
  size,
  recipe,
  photo,
}: {
  tool: RetouchTool;
  /** The shown picture's pixels. */
  size: { width: number; height: number };
  recipe: EditRecipe;
  /** The source photo's size. */
  photo: { width: number; height: number };
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const { width: W, height: H } = size;
  const crop: CropRect = recipe.geometry?.crop ?? FULL_CROP;
  const g = placementOf(recipe.geometry);
  const toSource = frameToSource(g, photo.width, photo.height);
  const toFrame = sourceToFrame(g, photo.width, photo.height);
  const long = Math.max(photo.width, photo.height);
  const aspect = photo.width / photo.height;
  const [pointer, setPointer] = useState<Point | null>(null);
  const drag = useRef<{ start: Point; spot: Spot; index: number; which: "spot" | "source"; frame: number | null; next: Spot | null } | null>(
    null,
  );

  // Screen pixels per photo long edge: the shown picture is the crop of the frame.
  const { setScale } = tool;
  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    const measure = () => {
      const frameWidth = orientedSize(g, photo.width, photo.height).width;
      setScale((box.getBoundingClientRect().width / (crop.w * frameWidth)) * long);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [setScale, crop.w, g.rotation, photo.width, photo.height, long]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTextEntry(e.target) || e.target instanceof HTMLInputElement) return;
      if ((e.key === "Delete" || e.key === "Backspace") && tool.selected !== null) {
        tool.remove(tool.selected);
        e.preventDefault();
      } else if (e.key === "Escape" && tool.selected !== null) {
        tool.select(null);
        e.preventDefault();
      } else if (e.key === "[" || e.key === "]") {
        const next = e.key === "[" ? tool.size / 1.15 : tool.size * 1.15;
        tool.setSize(Math.round(Math.min(BRUSH_SIZE.max, Math.max(BRUSH_SIZE.min, next))));
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tool]);

  /** A pointer event's place: in the shown picture's pixels, and in the source. */
  const at = (ev: ReactPointerEvent) => {
    const box = boxRef.current!.getBoundingClientRect();
    const u = (ev.clientX - box.left) / box.width;
    const v = (ev.clientY - box.top) / box.height;
    return { shown: [u * W, v * H] as Point, source: toSource(fromShown([u, v], crop)) };
  };
  /** A source point in the shown picture's pixels. */
  const shown = (p: Point): Point => {
    const s = toShown(toFrame(p), crop);
    return [s[0] * W, s[1] * H];
  };
  /** A spot's radius in the shown picture's pixels. */
  const radius = (s: Spot) => {
    const c = shown([s.x, s.y]);
    const e = shown([s.x + (s.radius * long) / photo.width, s.y]);
    return Math.max(Math.hypot(e[0] - c[0], e[1] - c[1]), 2);
  };

  const onPointerDown = (ev: ReactPointerEvent<HTMLDivElement>) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    const { source } = at(ev);
    const spots = tool.spots;
    // Option-click: the selected spot's source moves here.
    if (ev.altKey && tool.spot && tool.selected !== null) {
      tool.update(tool.selected, { ...tool.spot, sourceX: source[0], sourceY: source[1] });
      return;
    }
    const sourceHit = tool.selected !== null && spotAt([tool.spot!], source, aspect, true) !== null ? tool.selected : null;
    const hit = sourceHit ?? spotAt(spots, source, aspect);
    if (hit === null) {
      void tool.place(source);
      return;
    }
    tool.select(hit);
    ev.currentTarget.setPointerCapture(ev.pointerId);
    drag.current = { start: source, spot: spots[hit]!, index: hit, which: sourceHit !== null ? "source" : "spot", frame: null, next: null };
  };
  const onPointerMove = (ev: ReactPointerEvent<HTMLDivElement>) => {
    const p = at(ev);
    setPointer(p.shown);
    const d = drag.current;
    if (!d) return;
    d.next = moveSpot(d.spot, [p.source[0] - d.start[0], p.source[1] - d.start[1]], d.which);
    // At most one edit per display frame.
    d.frame ??= requestAnimationFrame(() => {
      const now = drag.current;
      if (!now) return;
      now.frame = null;
      if (now.next) tool.update(now.index, now.next);
    });
  };
  const onPointerUp = () => {
    const d = drag.current;
    if (!d) return;
    if (d.frame != null) cancelAnimationFrame(d.frame);
    if (d.next) tool.update(d.index, d.next);
    drag.current = null;
  };

  const brushRadius = (tool.size / 2) * (W / (boxRef.current?.getBoundingClientRect().width || W));
  const overSpot = pointer !== null && tool.spots.some((s) => Math.hypot(...minus(shown([s.x, s.y]), pointer)) <= radius(s));
  return (
    <div
      ref={boxRef}
      className="retouch-surface"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onPointerLeave={() => setPointer(null)}
    >
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
        {tool.spots.map((s, i) => {
          const c = shown([s.x, s.y]);
          const r = radius(s);
          if (i !== tool.selected) return <circle key={i} className="spot-ring" cx={c[0]} cy={c[1]} r={r} />;
          const from = shown([s.sourceX, s.sourceY]);
          const len = Math.hypot(...minus(from, c)) || 1;
          const u: Point = [(from[0] - c[0]) / len, (from[1] - c[1]) / len];
          return (
            <g key={i} className="spot-selected">
              {len > 2 * r && <line className="spot-link" x1={c[0] + u[0] * r} y1={c[1] + u[1] * r} x2={from[0] - u[0] * r} y2={from[1] - u[1] * r} />}
              <circle className="spot-ring source" cx={from[0]} cy={from[1]} r={r} />
              <circle className="spot-ring selected" cx={c[0]} cy={c[1]} r={r} />
            </g>
          );
        })}
        {pointer && !overSpot && !drag.current && <circle className="brush-ring" cx={pointer[0]} cy={pointer[1]} r={brushRadius} />}
      </svg>
    </div>
  );
}

const minus = (a: Point, b: Point): [number, number] => [a[0] - b[0], a[1] - b[1]];

const BRUSH_SPEC: AdjustmentSpec = {
  key: "brushSize",
  label: "Brush size",
  group: "Retouch",
  min: BRUSH_SIZE.min,
  max: BRUSH_SIZE.max,
  step: 1,
  default: BRUSH_SIZE.initial,
  more: false,
  unit: "px",
};

/** The Retouch section, as in the design: the tool, what it does, and the brush size;
 *  then the spots made, with a way to clear them. */
export function RetouchControls({ tool, disabled }: { tool: RetouchTool; disabled: boolean }) {
  const current = RETOUCH_TOOLS.find((t) => t.kind === tool.kind) ?? RETOUCH_TOOLS[0]!;
  const count = tool.spots.length;
  return (
    <div className="retouch">
      <div className="segmented small retouch-tools" role="radiogroup" aria-label="Retouch tool">
        {RETOUCH_TOOLS.map((t) => (
          <button key={t.kind} role="radio" aria-checked={tool.kind === t.kind} title={t.hint} disabled={disabled} onClick={() => tool.setKind(t.kind)}>
            {t.label}
          </button>
        ))}
      </div>
      <p className="retouch-hint">
        {current.text} {tool.spot ? "Changes here apply to the selected spot." : "Click the photo to add a spot."}
      </p>
      <Slider
        id="retouch-size"
        spec={BRUSH_SPEC}
        value={tool.size}
        shown={`${tool.size} px`}
        zeroMark={false}
        disabled={disabled}
        onChange={(v) => tool.setSize(v)}
      />
      {count > 0 && (
        <div className="retouch-count">
          <span>
            {count} {count === 1 ? "spot" : "spots"}
          </span>
          <button className="link-button" disabled={disabled} onClick={tool.clear}>
            Clear all
          </button>
        </div>
      )}
    </div>
  );
}
