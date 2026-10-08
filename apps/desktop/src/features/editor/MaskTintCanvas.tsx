import { useEffect, useRef, useState, type RefObject } from "react";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { Mask } from "../../ipc/generated/Mask";
import { MaskTint } from "./maskTint";
import { useViewerZoom } from "./viewerZoom";
import { drawnPart } from "./zoom";

/** The active mask's tint over the photo (see `MaskTint`), in device pixels. At 100 %
 *  (ADR 0070) only the part in view is drawn (with a margin), as a narrower crop: the
 *  whole photo would be a canvas of its full resolution. */
export function MaskTintCanvas({ mask, crop, boxRef }: { mask: Mask; crop: CropRect; boxRef: RefObject<HTMLDivElement | null> }) {
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
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || boxSize.width === 0) return;
    tint.current ??= new MaskTint();
    const colour = getComputedStyle(canvas).getPropertyValue("--accent").trim() || "#f0b45e";
    // The drawn part of the shown crop, as a crop of the frame.
    const part = { x: x + p0 * w, y: y + q0 * h, w: (p1 - p0) * w, h: (q1 - q0) * h };
    tint.current.draw(canvas, mask, { width, height, crop: part }, colour);
  }, [mask, boxSize, width, height, x, y, w, h, p0, q0, p1, q1]);
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
