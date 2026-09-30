import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { CropIcon, LevelIcon } from "../../components/icons";
import * as ipc from "../../ipc/client";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { AspectRatio } from "../../ipc/generated/AspectRatio";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { Geometry } from "../../ipc/generated/Geometry";
import {
  ASPECTS,
  FULL,
  aspectRatioOf,
  cropView,
  drag,
  fromView,
  largestIn,
  remap,
  toView,
  viewRatio,
  type Handle,
  type ViewShape,
} from "./cropGeometry";
import { GroupSliders } from "./AdjustmentPanel";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";

const NO_GEOMETRY: Geometry = { straighten: 0, crop: FULL, aspect: "original", vertical: 0, horizontal: 0 };

export function geometryOf(r: EditRecipe): Geometry {
  return r.geometry ?? NO_GEOMETRY;
}

/**
 * The crop and straighten tool's state and actions (ADR 0032). The recipe holds the
 * geometry; the tool shows the whole straightened view while it is open.
 */
export function useCropTool(opts: {
  recipe: EditRecipe | null;
  imageId: number | null;
  size: { width: number; height: number } | null;
  onChange: (r: EditRecipe) => void;
  setViewTransform: (t: ((r: EditRecipe) => EditRecipe) | null) => void;
}) {
  const { recipe, imageId, size, onChange, setViewTransform } = opts;
  const [open, setOpen] = useState(false);
  // Auto level's result, shown for a moment ("Horizon levelled · −1.4°").
  const [status, setStatus] = useState<string | null>(null);
  useEffect(() => {
    if (status === null) return;
    const t = window.setTimeout(() => setStatus(null), 3000);
    return () => window.clearTimeout(t);
  }, [status]);
  const g = recipe ? geometryOf(recipe) : NO_GEOMETRY;
  const view = size ? cropView(g, size.width, size.height) : FULL;

  useEffect(() => {
    if (!open || !size) {
      setViewTransform(null);
      return;
    }
    // While cropping, render the whole straightened (and corrected) view and draw the
    // crop over it.
    setViewTransform((r) => {
      const geo = geometryOf(r);
      return { ...r, geometry: { ...geo, crop: cropView(geo, size.width, size.height), aspect: "free" } };
    });
    return () => setViewTransform(null);
  }, [open, size, setViewTransform]);

  const update = useCallback(
    (next: Geometry) => {
      if (recipe) onChange({ ...recipe, geometry: next });
    },
    [recipe, onChange],
  );

  // A new view shape keeps the crop in the same place relative to the view, so it
  // stays inside the photo.
  const setShape = (change: Partial<ViewShape>) => {
    if (!size) return;
    const shaped = { ...g, ...change };
    const next = cropView(shaped, size.width, size.height);
    update({ ...shaped, crop: remap(g.crop, view, next) });
  };
  const setStraighten = (straighten: number) => setShape({ straighten });

  const ratioInView = (a: AspectRatio) => {
    const r = size ? aspectRatioOf(a, size.width, size.height) : null;
    return r !== null && size ? viewRatio(r, view, size.width, size.height) : null;
  };

  return {
    open,
    geometry: g,
    /** The crop in fractions of the view on screen. */
    overlay: toView(g.crop, view),
    overlayRatio: ratioInView(g.aspect),
    pixelSize: size ? { width: Math.round(g.crop.w * size.width), height: Math.round(g.crop.h * size.height) } : null,
    enter: () => setOpen(true),
    done: () => setOpen(false),
    reset: () => {
      if (recipe) onChange({ ...recipe, geometry: undefined });
    },
    setAspect: (aspect: AspectRatio) => {
      const r = ratioInView(aspect);
      const crop = r === null ? g.crop : fromView(largestIn(r), view);
      update({ ...g, aspect, crop });
    },
    setStraighten,
    /** Vertical or Horizontal perspective (ADR 0034). */
    setPerspective: (key: string, value: number) => {
      if (key === "vertical" || key === "horizontal") setShape({ [key]: value });
    },
    status,
    /** Levels the photo from its horizon or verticals, found by the renderer. */
    autoLevel: () => {
      if (imageId === null) return;
      ipc
        .autoLevel(imageId)
        .then((angle) => {
          if (angle === null) {
            setStatus("No clear horizon found");
            return;
          }
          setStraighten(angle);
          setStatus(`Horizon levelled · ${angle > 0 ? "+" : angle < 0 ? "−" : ""}${Math.abs(angle).toFixed(1)}°`);
        })
        .catch(() => setStatus("Couldn't level the photo"));
    },
    setOverlay: (o: CropRect) => update({ ...g, crop: fromView(o, view) }),
  };
}

export type CropTool = ReturnType<typeof useCropTool>;

const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

/** The crop rectangle over the photo, as in the design: the outside dimmed, a thirds
 *  grid, corner and edge handles; drag inside to move. */
export function CropOverlay({ tool }: { tool: CropTool }) {
  const boxRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ handle: Handle; start: CropRect; x: number; y: number } | null>(null);

  const begin = (handle: Handle) => (e: ReactPointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);
    dragRef.current = { handle, start: tool.overlay, x: e.clientX, y: e.clientY };
  };
  const move = (e: ReactPointerEvent) => {
    const d = dragRef.current;
    const box = boxRef.current?.getBoundingClientRect();
    if (!d || !box) return;
    tool.setOverlay(drag(d.start, d.handle, (e.clientX - d.x) / box.width, (e.clientY - d.y) / box.height, tool.overlayRatio));
  };
  const end = () => {
    dragRef.current = null;
  };

  const o = tool.overlay;
  const pct = (v: number) => `${(v * 100).toFixed(3)}%`;
  return (
    <div className="crop-overlay" ref={boxRef} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
      <div
        className="crop-rect"
        style={{ left: pct(o.x), top: pct(o.y), width: pct(o.w), height: pct(o.h) }}
        onPointerDown={begin("move")}
      >
        <span className="crop-third v1" />
        <span className="crop-third v2" />
        <span className="crop-third h1" />
        <span className="crop-third h2" />
        {HANDLES.map((h) => (
          <span key={h} className={`crop-handle ${h}`} onPointerDown={begin(h)} />
        ))}
      </div>
      {tool.pixelSize && (
        <span className="crop-size">
          {tool.pixelSize.width} × {tool.pixelSize.height}
        </span>
      )}
    </div>
  );
}

/** The floating toolbar while cropping, as in the design. */
export function CropToolbar({ tool, straighten }: { tool: CropTool; straighten: AdjustmentSpec }) {
  useEffect(() => {
    // Enter or Escape finishes, as Done does.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === "Escape") tool.done();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tool]);
  const value = tool.geometry.straighten;
  return (
    <div className="photo-toolbar crop-toolbar" role="toolbar" aria-label="Crop">
      <AspectButtons tool={tool} className="crop-ratio" />
      <span className="toolbar-divider" />
      <label htmlFor="crop-straighten" className="crop-label">
        Straighten
      </label>
      <input
        id="crop-straighten"
        className="range crop-straighten"
        type="range"
        min={straighten.min}
        max={straighten.max}
        step={straighten.step}
        value={value}
        onChange={(e) => tool.setStraighten(Number(e.currentTarget.value))}
        onDoubleClick={() => tool.setStraighten(0)}
      />
      <span className="crop-angle">{formatSliderValue(value, straighten.min, straighten.step, straighten.unit)}</span>
      <button className="crop-reset" onClick={tool.autoLevel}>
        Auto level
      </button>
      <span className="toolbar-divider" />
      <button className="crop-reset" onClick={tool.reset}>
        Reset
      </button>
      <button className="primary crop-done" onClick={tool.done}>
        Done
      </button>
      {tool.status && (
        <span className="crop-status" role="status">
          {tool.status}
        </span>
      )}
    </div>
  );
}

function AspectButtons({ tool, className }: { tool: CropTool; className: string }) {
  return (
    <>
      {ASPECTS.map((a) => (
        <button key={a.id} className={className} aria-pressed={tool.geometry.aspect === a.id} onClick={() => tool.setAspect(a.id)}>
          {a.label}
        </button>
      ))}
    </>
  );
}

/** The panel's Geometry section body, as in the design: aspect ratio, Straighten, a
 *  way into the crop tool, and "More controls" headed "Perspective & lens". */
export function GeometryControls({
  tool,
  straighten,
  perspective,
  lens,
  disabled,
}: {
  tool: CropTool;
  straighten: AdjustmentSpec;
  perspective: AdjustmentSpec[];
  /** The lens switch, last in "Perspective & lens", and whether it is on. */
  lens: { content: ReactNode; edited: boolean };
  disabled: boolean;
}) {
  const value = tool.geometry.straighten;
  const shown = useMemo(
    () => formatSliderValue(value, straighten.min, straighten.step, straighten.unit),
    [value, straighten],
  );
  return (
    <>
      <div className="geometry-aspect">
        <span className="geometry-label">Aspect ratio</span>
        <div className="aspect-grid">
          <AspectButtons tool={tool} className="aspect-button" />
        </div>
      </div>
      <Slider
        id="geo-straighten"
        spec={straighten}
        value={value}
        shown={shown}
        disabled={disabled}
        onChange={tool.setStraighten}
      />
      <div className="geometry-actions">
        <button className="chip geometry-crop" disabled={disabled} onClick={tool.autoLevel}>
          <LevelIcon size={14} />
          Auto level
        </button>
        <button className="chip geometry-crop" disabled={disabled} onClick={tool.enter}>
          <CropIcon size={14} />
          Crop
        </button>
      </div>
      {tool.status && (
        <span className="geometry-status" role="status">
          {tool.status}
        </span>
      )}
      <GroupSliders
        specs={perspective}
        valueOf={(s) => (s.key === "vertical" ? tool.geometry.vertical : s.key === "horizontal" ? tool.geometry.horizontal : s.default)}
        format={(s, v) => formatSliderValue(v, s.min, s.step, s.unit)}
        extra={{ edited: false, before: true, content: <div className="more-title">Perspective &amp; lens</div> }}
        after={lens}
        disabled={disabled}
        onChange={tool.setPerspective}
      />
    </>
  );
}
