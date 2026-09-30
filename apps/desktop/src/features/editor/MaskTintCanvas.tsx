import { useEffect, useRef, useState, type RefObject } from "react";
import type { CropRect } from "../../ipc/generated/CropRect";
import type { Mask } from "../../ipc/generated/Mask";
import { MaskTint } from "./maskTint";

/** The active mask's tint over the photo (see `MaskTint`), in device pixels. */
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
  const { x, y, w, h } = crop;
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || boxSize.width === 0) return;
    tint.current ??= new MaskTint();
    const colour = getComputedStyle(canvas).getPropertyValue("--accent").trim() || "#f0b45e";
    tint.current.draw(canvas, mask, { width: boxSize.width, height: boxSize.height, crop: { x, y, w, h } }, colour);
  }, [mask, boxSize, x, y, w, h]);
  return <canvas ref={canvasRef} className="mask-tint-canvas" aria-hidden="true" />;
}
