import { useEffect, useRef } from "react";
import type { DisplayedFrame } from "./useEditor";

interface Props {
  displayed: DisplayedFrame | null;
  /** Reports the viewport's long edge in device pixels. */
  onResize: (longEdgeDevicePx: number) => void;
  placeholder: string;
}

/** Blits Rust-rendered RGBA frames to a canvas. No pixel processing happens here. */
export function Viewer({ displayed, onResize, placeholder }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      onResize(Math.max(width, height) * window.devicePixelRatio);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [onResize]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !displayed) return;
    const { frame } = displayed;
    if (canvas.width !== frame.width || canvas.height !== frame.height) {
      canvas.width = frame.width;
      canvas.height = frame.height;
    }
    const ctx = canvas.getContext("2d");
    ctx?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  }, [displayed]);

  return (
    <div className="viewer" ref={containerRef}>
      {displayed ? <canvas ref={canvasRef} className="viewer-canvas" /> : <p className="viewer-empty">{placeholder}</p>}
    </div>
  );
}
