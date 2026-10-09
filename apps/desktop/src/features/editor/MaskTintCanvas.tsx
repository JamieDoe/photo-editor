import { useEffect, useRef, useState, type RefObject } from "react";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { Geometry } from "../../ipc/generated/Geometry";
import type { Mask } from "../../ipc/generated/Mask";
import { maskView } from "../../ipc/client";
import { MaskTint, coverageCanvas, type GeneratedView } from "./maskTint";
import { shapesOf } from "./masks";
import { useViewerZoom } from "./viewerZoom";
import { drawnPart } from "./zoom";

/** The active mask's tint over the photo (see `MaskTint`), in device pixels. At 100 %
 *  (ADR 0070) only the part in view is drawn (with a margin), as a narrower crop: the
 *  whole photo would be a canvas of its full resolution. */
export function MaskTintCanvas({
  mask,
  crop,
  boxRef,
  imageId,
  geometry,
  profileCorrections,
}: {
  mask: Mask;
  crop: CropRect;
  boxRef: RefObject<HTMLDivElement | null>;
  imageId: number | null;
  geometry: Geometry | null;
  /** Whether the recipe applies the lens's profile (ADR 0075), which moves the frame. */
  profileCorrections: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tint = useRef<MaskTint | null>(null);
  // The canvas matches the photo's box on screen.
  const [boxSize, setBoxSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const dpr = window.devicePixelRatio;
      setBoxSize({
        width: Math.round(entry.contentRect.width * dpr),
        height: Math.round(entry.contentRect.height * dpr),
      });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [boxRef]);
  const { visible } = useViewerZoom();
  const [p0, q0, p1, q1] = visible ? drawnPart(visible) : [0, 0, 1, 1];
  const width = Math.max(1, Math.round(boxSize.width * (p1 - p0)));
  const height = Math.max(1, Math.round(boxSize.height * (q1 - q0)));
  const { x, y, w, h } = crop;
  const generated = useGeneratedViews(
    mask,
    crop,
    boxSize.width / Math.max(boxSize.height, 1),
    imageId,
    geometry,
    profileCorrections,
  );
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || boxSize.width === 0) return;
    tint.current ??= new MaskTint();
    const colour = getComputedStyle(canvas).getPropertyValue("--accent").trim() || "#f0b45e";
    // The drawn part of the shown crop, as a crop of the frame.
    const part = { x: x + p0 * w, y: y + q0 * h, w: (p1 - p0) * w, h: (q1 - q0) * h };
    tint.current.draw(canvas, mask, { width, height, crop: part }, colour, generated);
  }, [mask, boxSize, width, height, x, y, w, h, p0, q0, p1, q1, generated]);
  const pct = (v: number) => `${v * 100}%`;
  return (
    <canvas
      ref={canvasRef}
      className="mask-tint-canvas"
      aria-hidden="true"
      style={{ left: pct(p0), top: pct(q0), width: pct(p1 - p0), height: pct(q1 - q0) }}
    />
  );
}

/** Generated masks' views are made once over the whole shown crop at this long edge,
 *  then scaled for zooming and panning: smooth coverage scales well. */
const GENERATED_VIEW_EDGE = 2048;

/**
 * The views of `mask`'s generated shapes (ADR 0074) over `crop` of the frame, made by
 * the engine (which maps them as the renderer does), by name. `aspect` is the shown
 * crop's on screen. Kept until the photo, the frame, the crop or the names change.
 */
function useGeneratedViews(
  mask: Mask,
  crop: CropRect,
  aspect: number,
  imageId: number | null,
  geometry: Geometry | null,
  profileCorrections: boolean,
): ReadonlyMap<string, GeneratedView> {
  const names = shapesOf(mask)
    .flatMap(({ shape }) => (shape.kind === "generated" ? [shape.mask] : []))
    .join(",");
  const [views, setViews] = useState<{ of: string; views: ReadonlyMap<string, GeneratedView> }>({ of: "", views: new Map() });
  const width = aspect >= 1 ? GENERATED_VIEW_EDGE : Math.max(1, Math.round(GENERATED_VIEW_EDGE * aspect));
  const height = aspect >= 1 ? Math.max(1, Math.round(GENERATED_VIEW_EDGE / aspect)) : GENERATED_VIEW_EDGE;
  const geometryKey = JSON.stringify(geometry);
  // What the views are of; the size they are made at only sharpens them.
  const of = `${imageId}|${geometryKey}|${profileCorrections}|${names}`;
  const key = `${of}|${width}x${height}`;
  useEffect(() => {
    if (imageId === null || names === "" || !Number.isFinite(aspect) || aspect <= 0) return;
    let stale = false;
    const viewCrop = { ...crop };
    void Promise.all(
      names.split(",").map(async (name) => {
        const view = { name, geometry, profileCorrections, crop: viewCrop, width, height };
        const bytes = await maskView(imageId, view).catch(() => new Uint8Array());
        return [name, bytes] as const;
      }),
    ).then((made) => {
      if (stale) return;
      const next = new Map<string, GeneratedView>();
      for (const [name, bytes] of made) {
        if (bytes.length === width * height) next.set(name, { canvas: coverageCanvas(bytes, width, height), crop: viewCrop });
      }
      setViews({ of, views: next });
    });
    return () => {
      stale = true;
    };
    // The geometry and crop are in the key.
  }, [key, crop.x, crop.y, crop.w, crop.h]);
  return views.of === of ? views.views : EMPTY;
}

const EMPTY: ReadonlyMap<string, GeneratedView> = new Map();
