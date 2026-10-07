import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { CheckIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Spot } from "../../ipc/generated/Spot";
import type { SpotKind } from "../../ipc/generated/SpotKind";
import { findDust, newSpot } from "../../ipc/client";
import { isTextEntry } from "../../lib/keyboard";
import { orientedSize } from "./cropGeometry";
import { FULL_CROP, fromShown, toShown, type Point } from "./masks";
import { Slider } from "./Slider";
import {
  BRUSH_SIZE,
  RETOUCH_TOOLS,
  frameToSource,
  moveSpot,
  newRemoval,
  placementOf,
  removalAt,
  removalsOf,
  sourceToFrame,
  spotAt,
  spotsOf,
  stillToFix,
  strokeRadius,
  strokeSizeFor,
  withRemovals,
  withSpots,
  withoutSpots,
  type RetouchToolKind,
} from "./spots";

/**
 * Retouch mode (ADRs 0054, 0066): the tool (Remove paints areas to fill; Heal and
 * Clone place spots) and the brush size, the selected spot (which the panel's controls
 * change instead) or removal, and the edits the overlay and panel make. The spots and
 * removals themselves live in the recipe.
 */
export function useRetouchTool(opts: {
  recipe: EditRecipe | null;
  imageId: number | null;
  /** The photo's width / height, for telling which spots cover which. */
  aspect: number;
  /** Retouch mode is on: sensor dust is looked for (once per photo). */
  active: boolean;
  onChange: (r: EditRecipe) => void;
  notify: (message: string) => void;
}) {
  const { recipe, imageId, aspect, active, onChange, notify } = opts;
  // Remove first, as the design has it.
  const [kind, setKindState] = useState<RetouchToolKind>("remove");
  const [size, setSizeState] = useState<number>(BRUSH_SIZE.initial);
  const [selected, setSelected] = useState<number | null>(null);
  const [selectedRemoval, setSelectedRemoval] = useState<number | null>(null);
  // Screen pixels per unit of the photo's long edge, as the overlay last measured.
  const [scale, setScale] = useState(1000);
  const spots = recipe ? spotsOf(recipe) : [];
  const spot = selected !== null ? (spots[selected] ?? null) : null;
  const removals = recipe ? removalsOf(recipe) : [];
  // The latest recipe, for edits that finish after an await.
  const latest = useRef(recipe);
  latest.current = recipe;

  // Sensor dust (ADR 0058): what was found on this photo, and the spots the last Fix
  // all made (for its Undo).
  const [dust, setDust] = useState<{ imageId: number; found: Spot[] | null } | null>(null);
  const [fixedAll, setFixedAll] = useState<Spot[] | null>(null);
  useEffect(() => {
    if (!active || imageId === null || dust?.imageId === imageId) return;
    setDust({ imageId, found: null });
    setFixedAll(null);
    findDust(imageId, latest.current ? spotsOf(latest.current) : []).then(
      (found) => setDust((d) => (d?.imageId === imageId ? { imageId, found } : d)),
      () => setDust((d) => (d?.imageId === imageId ? { imageId, found: [] } : d)),
    );
  }, [active, imageId, dust?.imageId]);
  const found = dust?.imageId === imageId ? dust.found : null;
  const toFix = found ? stillToFix(found, spots, aspect) : [];

  useEffect(() => {
    setSelected(null);
    setSelectedRemoval(null);
  }, [imageId]);
  useEffect(() => {
    if (selected !== null && selected >= spots.length) setSelected(null);
  }, [selected, spots.length]);
  useEffect(() => {
    if (selectedRemoval !== null && selectedRemoval >= removals.length) setSelectedRemoval(null);
  }, [selectedRemoval, removals.length]);

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
    removals,
    selectedRemoval,
    kind: spot?.kind ?? kind,
    /** The brush size in screen pixels: the selected spot's, or the next one's. */
    size: spot ? Math.round(spot.radius * 2 * scale) : size,
    scale,
    setScale,
    select: (i: number | null) => {
      setSelected(i);
      if (i !== null) setSelectedRemoval(null);
    },
    selectRemoval: (i: number | null) => {
      setSelectedRemoval(i);
      if (i !== null) setSelected(null);
    },
    setKind: (k: RetouchToolKind) => {
      setKindState(k);
      if (spot && selected !== null) {
        if (k === "remove") setSelected(null);
        else update(selected, { ...spot, kind: k });
      }
    },
    setSize: (px: number) => {
      setSizeState(px);
      if (spot && selected !== null) update(selected, { ...spot, radius: radiusFor(px) });
    },
    /** A new spot at source point `at`, its source found by the engine. */
    place: async (at: Point) => {
      if (imageId === null || !latest.current) return;
      const existing = spotsOf(latest.current);
      const spotKind: SpotKind = kind === "remove" ? "heal" : kind;
      const made = await newSpot(imageId, spotKind, at, radiusFor(size), existing).catch(() => null);
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
    /** A new removal painted along `points` (source fractions), at the brush's size. */
    addRemoval: (points: readonly Point[]) => {
      if (!latest.current || points.length === 0) return;
      const removal = newRemoval(points, strokeSizeFor(size, scale, aspect));
      onChange(withRemovals(latest.current, [...removalsOf(latest.current), removal]));
      setSelected(null);
      setSelectedRemoval(null);
    },
    deleteRemoval: (i: number) => {
      if (latest.current) onChange(withRemovals(latest.current, removalsOf(latest.current).filter((_, j) => j !== i)));
      setSelectedRemoval(null);
    },
    /** Every spot and removal gone, in one edit. */
    clear: () => {
      if (latest.current) onChange(withRemovals(withSpots(latest.current, []), []));
      setSelected(null);
      setSelectedRemoval(null);
    },
    /** Sensor dust still to fix (ADR 0058); null while it is being looked for. */
    dust: found === null ? null : toFix,
    /** Heals every dust spot still to fix, in one edit. */
    fixAll: () => {
      if (toFix.length === 0) return;
      commit([...spots, ...toFix]);
      setFixedAll(toFix);
      setSelected(null);
    },
    /** Heals one dust spot. */
    fixOne: (d: Spot) => {
      commit([...spots, d]);
      setFixedAll(null);
    },
    /** The spots the last Fix all made, while its Undo is offered. */
    fixedAll,
    undoFixAll: () => {
      if (fixedAll) commit(withoutSpots(spots, fixedAll));
      setFixedAll(null);
    },
  };
}

export type RetouchTool = ReturnType<typeof useRetouchTool>;

/**
 * The spots and removals on the photo. Spots are dashed circles in the design's
 * amber, the selected one white with its source joined to it; removals are a small
 * pin where each was started, the selected one showing the area painted.
 *
 * With Remove, drag to paint over something (or click for a dab): on release it is
 * filled in. Click a painted area to select it. With Heal or Clone, click to add a
 * spot. Drag a spot or its source to move it; Option-click to take the selected spot
 * from there. Delete removes the selected spot or removal.
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
  // A Remove stroke being painted: its points in the source and as shown, and whether
  // the pointer has moved (a still click on a painted area selects it instead).
  const paint = useRef<{ source: Point[]; last: [number, number]; moved: boolean } | null>(null);
  const [painted, setPainted] = useState<Point[] | null>(null);

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
      } else if ((e.key === "Delete" || e.key === "Backspace") && tool.selectedRemoval !== null) {
        tool.deleteRemoval(tool.selectedRemoval);
        e.preventDefault();
      } else if (e.key === "Escape" && (tool.selected !== null || tool.selectedRemoval !== null)) {
        tool.select(null);
        tool.selectRemoval(null);
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
  /** A radius (a fraction of the photo's long edge) around source point `p`, in the
   *  shown picture's pixels. */
  const shownRadius = (p: Point, r: number) => {
    const c = shown(p);
    const e = shown([p[0] + (r * long) / photo.width, p[1]]);
    return Math.max(Math.hypot(e[0] - c[0], e[1] - c[1]), 2);
  };
  /** A spot's radius in the shown picture's pixels. */
  const radius = (s: Spot) => shownRadius([s.x, s.y], s.radius);
  const removing = tool.kind === "remove";

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
    if (hit === null && removing) {
      ev.currentTarget.setPointerCapture(ev.pointerId);
      paint.current = { source: [source], last: [ev.clientX, ev.clientY], moved: false };
      setPainted([source]);
      return;
    }
    if (hit === null) {
      // A click on found dust heals it, at the size found.
      const dust = tool.dust ?? [];
      const onDust = spotAt(dust, source, aspect);
      if (onDust !== null) tool.fixOne(dust[onDust]!);
      else void tool.place(source);
      return;
    }
    tool.select(hit);
    ev.currentTarget.setPointerCapture(ev.pointerId);
    drag.current = { start: source, spot: spots[hit]!, index: hit, which: sourceHit !== null ? "source" : "spot", frame: null, next: null };
  };
  const onPointerMove = (ev: ReactPointerEvent<HTMLDivElement>) => {
    const p = at(ev);
    setPointer(p.shown);
    const stroke = paint.current;
    if (stroke) {
      // New points two screen pixels apart at least; shown as painted, filled on release.
      if (Math.hypot(ev.clientX - stroke.last[0], ev.clientY - stroke.last[1]) >= 2) {
        stroke.source.push(p.source);
        stroke.last = [ev.clientX, ev.clientY];
        stroke.moved = true;
        setPainted([...stroke.source]);
      }
      return;
    }
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
    const stroke = paint.current;
    if (stroke) {
      paint.current = null;
      setPainted(null);
      // A still click on a painted area selects it; anything else is a new removal.
      const onRemoval = stroke.moved ? null : removalAt(tool.removals, stroke.source[0]!, aspect);
      if (onRemoval !== null) tool.selectRemoval(onRemoval);
      else tool.addRemoval(stroke.source);
      return;
    }
    const d = drag.current;
    if (!d) return;
    if (d.frame != null) cancelAnimationFrame(d.frame);
    if (d.next) tool.update(d.index, d.next);
    drag.current = null;
  };

  // The picture's pixels per screen pixel: rings and pins are sized on screen.
  const perScreenPixel = W / (boxRef.current?.getBoundingClientRect().width || W);
  const brushRadius = (tool.size / 2) * perScreenPixel;
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
        {tool.removals.map((m, i) => {
          const first = m.strokes[0]?.points[0];
          if (!first) return null;
          return (
            <g key={`removal-${i}`}>
              {i === tool.selectedRemoval &&
                m.strokes.map((s, k) => (
                  <PaintedPath key={k} points={s.points.map((q) => shown(q as Point))} width={2 * shownRadius(s.points[0] as Point, strokeRadius(s, aspect))} className="removal-area" />
                ))}
              <circle className={i === tool.selectedRemoval ? "removal-pin selected" : "removal-pin"} cx={shown(first as Point)[0]} cy={shown(first as Point)[1]} r={5 * perScreenPixel} />
            </g>
          );
        })}
        {painted && painted.length > 0 && (
          <PaintedPath points={painted.map(shown)} width={2 * brushRadius} className="removal-paint" />
        )}
        {(tool.dust ?? []).map((d, i) => {
          const c = shown([d.x, d.y]);
          return <circle key={`dust-${i}`} className="dust-ring" cx={c[0]} cy={c[1]} r={Math.max(radius(d), 9)} />;
        })}
        {pointer && !overSpot && !drag.current && <circle className="brush-ring" cx={pointer[0]} cy={pointer[1]} r={brushRadius} />}
      </svg>
    </div>
  );
}

const minus = (a: Point, b: Point): [number, number] => [a[0] - b[0], a[1] - b[1]];

/** A painted stroke: its path drawn as wide as the brush, round at the ends. */
function PaintedPath({ points, width, className }: { points: Point[]; width: number; className: string }) {
  if (points.length === 1) return <circle className={className} cx={points[0]![0]} cy={points[0]![1]} r={width / 2} />;
  const d = points.map((p, i) => `${i === 0 ? "M" : "L"}${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join(" ");
  return <path className={className} d={d} strokeWidth={width} />;
}

/** The design's dust icon: a dashed ring around a speck. */
function DustIcon() {
  return (
    <svg className="icon dust-icon" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="6" strokeDasharray="2.5 2" />
      <circle cx="8" cy="8" r="1.5" />
    </svg>
  );
}

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
 *  sensor dust found, with Fix all (and its Undo); then the spots made, with a way to
 *  clear them. */
export function RetouchControls({ tool, disabled }: { tool: RetouchTool; disabled: boolean }) {
  const current = RETOUCH_TOOLS.find((t) => t.kind === tool.kind) ?? RETOUCH_TOOLS[0]!;
  const spotCount = tool.spots.length;
  const removalCount = tool.removals.length;
  const next =
    tool.kind === "remove"
      ? tool.selectedRemoval !== null
        ? "Delete removes the selected area."
        : "Drag over something to remove it."
      : tool.spot
        ? "Changes here apply to the selected spot."
        : "Click the photo to add a spot.";
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
        {current.text} {next}
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
      {tool.dust && tool.dust.length > 0 && (
        <div className="dust-found">
          <DustIcon />
          <span className="grow">
            {tool.dust.length} sensor dust {tool.dust.length === 1 ? "spot" : "spots"} found
          </span>
          <button className="dust-fix" disabled={disabled} onClick={tool.fixAll}>
            Fix all
          </button>
        </div>
      )}
      {tool.fixedAll && tool.dust?.length === 0 && (
        <div className="dust-fixed">
          <CheckIcon size={14} />
          {tool.fixedAll.length} {tool.fixedAll.length === 1 ? "spot" : "spots"} removed ·
          <button className="link-button" disabled={disabled} onClick={tool.undoFixAll}>
            Undo
          </button>
        </div>
      )}
      {spotCount + removalCount > 0 && (
        <div className="retouch-count">
          <span>
            {[
              removalCount > 0 && `${removalCount} ${removalCount === 1 ? "removal" : "removals"}`,
              spotCount > 0 && `${spotCount} ${spotCount === 1 ? "spot" : "spots"}`,
            ]
              .filter(Boolean)
              .join(" · ")}
          </span>
          <button className="link-button" disabled={disabled} onClick={tool.clear}>
            Clear all
          </button>
        </div>
      )}
    </div>
  );
}
