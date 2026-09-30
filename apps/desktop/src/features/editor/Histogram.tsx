import { useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import type { FrameHistogram } from "../../ipc/frame";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import { GRAPH_HEIGHT, GRAPH_WIDTH, ZONES, draggedValue, histogramPaths, zoneAt } from "./histogramGraph";
import { isAdjustmentKey } from "./recipe";
import { formatSliderValue } from "./sliderTrack";

interface Props {
  /** The shown frame's histogram; null before the first render. */
  histogram: FrameHistogram | null;
  specs: AdjustmentSpec[];
  recipe: EditRecipe | null;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
  /** The exposure details shown under the graph while it is not being used. */
  details: string[];
  emptyDetails: string;
}

/**
 * The panel's histogram, as in the design, that can be dragged as in Lightroom
 * (ADR 0036): its width is split into Blacks, Shadows, Exposure, Highlights and
 * Whites, and dragging across a zone moves that slider. Hovering names the zone and
 * its value where the exposure details are; a double-click resets it.
 */
export function Histogram({ histogram, specs, recipe, onChange, disabled, details, emptyDetails }: Props) {
  const paths = useMemo(() => (histogram ? histogramPaths(histogram) : null), [histogram]);
  const [hover, setHover] = useState<string | null>(null);
  const drag = useRef<{ key: string; x: number; start: number; width: number } | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);

  const spec = (key: string | null) => (key ? (specs.find((s) => s.key === key) ?? null) : null);
  const valueOf = (key: string): number | null => (recipe && isAdjustmentKey(key) ? recipe[key] : null);
  const zoneFor = (e: ReactPointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    return zoneAt((e.clientX - box.left) / box.width);
  };
  const interactive = !disabled && recipe !== null;

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!interactive || e.button !== 0) return;
    const key = zoneFor(e);
    const start = valueOf(key);
    if (start === null) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { key, x: e.clientX, start, width: e.currentTarget.getBoundingClientRect().width };
    setDragging(key);
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) {
      if (interactive) setHover(zoneFor(e));
      return;
    }
    const s = spec(d.key);
    if (!s || !recipe || !isAdjustmentKey(d.key)) return;
    const next = draggedValue(s, d.start, (e.clientX - d.x) / d.width);
    if (next !== recipe[d.key]) onChange({ ...recipe, [d.key]: next });
  };
  const endDrag = () => {
    drag.current = null;
    setDragging(null);
  };
  const onDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    if (!interactive) return;
    const box = e.currentTarget.getBoundingClientRect();
    const key = zoneAt((e.clientX - box.left) / box.width);
    const s = spec(key);
    if (s && recipe && isAdjustmentKey(key)) onChange({ ...recipe, [key]: s.default });
  };

  const active = dragging ?? hover;
  const activeSpec = spec(active);
  const activeValue = active ? valueOf(active) : null;
  const zone = ZONES.find((z) => z.key === active);

  return (
    <div className="panel-top">
      <div
        className={`histogram${interactive ? " interactive" : ""}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onPointerLeave={() => setHover(null)}
        onDoubleClick={onDoubleClick}
      >
        <svg
          viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`}
          preserveAspectRatio="none"
          role="img"
          aria-label="Histogram. Drag across it to adjust Blacks, Shadows, Exposure, Highlights or Whites."
        >
          {zone && (
            <rect
              className="histogram-zone"
              x={zone.from * GRAPH_WIDTH}
              y={0}
              width={(zone.to - zone.from) * GRAPH_WIDTH}
              height={GRAPH_HEIGHT}
            />
          )}
          {[72, 144, 216].map((x) => (
            <line key={x} className="histogram-grid" x1={x} y1={0} x2={x} y2={GRAPH_HEIGHT} />
          ))}
          {paths && (
            <>
              <path className="histogram-blue" d={paths.blue} />
              <path className="histogram-green" d={paths.green} />
              <path className="histogram-red" d={paths.red} />
              <path className="histogram-luma" d={paths.luma} />
            </>
          )}
        </svg>
        <span className={`clip-mark low${paths?.shadowsClipped ? " lit" : ""}`} title="Shadow clipping" />
        <span className={`clip-mark high${paths?.highlightsClipped ? " lit" : ""}`} title="Highlight clipping" />
      </div>
      <div className={active || details.length > 0 ? "panel-exif" : "panel-exif empty"} aria-live="polite">
        {activeSpec && activeValue !== null ? (
          <>
            <span className="histogram-readout-label">{activeSpec.label}</span>
            <span className="histogram-readout-value">
              {formatSliderValue(activeValue, activeSpec.min, activeSpec.step, activeSpec.unit)}
            </span>
          </>
        ) : details.length > 0 ? (
          details.map((x) => <span key={x}>{x}</span>)
        ) : (
          emptyDetails
        )}
      </div>
    </div>
  );
}
