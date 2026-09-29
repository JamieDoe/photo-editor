import { useEffect, useRef, useState } from "react";
import type { DisplayedFrame } from "./useEditor";
import { fitSize, nextBoxShape, type BoxShape } from "./viewerLayout";

interface Props {
  displayed: DisplayedFrame | null;
  /** The open image: its full size gives renders their exact shape. */
  image: { id: number; fullWidth: number; fullHeight: number } | null;
  /** Another photo is opening: the current one stays, dimmed, until it arrives. */
  loading: boolean;
  /** Reports the viewport's long edge in device pixels. */
  onResize: (longEdgeDevicePx: number) => void;
  placeholder: string;
}

/**
 * Blits Rust-rendered RGBA frames to a canvas. No pixel processing happens here. The
 * canvas is sized to fit the viewer (not to the frame's pixel count), so a quick
 * low-resolution frame and the later sharp one appear at the same size.
 */
export function Viewer({ displayed, image, loading, onResize, placeholder }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [space, setSpace] = useState({ width: 0, height: 0 });
  // The box shape is fixed per opening (see nextBoxShape), so frames never resize it.
  const [shape, setShape] = useState<BoxShape | null>(null);
  const imageRef = useRef(image);
  imageRef.current = image;
  useEffect(() => {
    if (!displayed) return;
    const img = imageRef.current;
    const full = img && img.id === displayed.imageId ? { width: img.fullWidth, height: img.fullHeight } : null;
    setShape((prev) => nextBoxShape(prev, displayed.imageId, displayed.frame, full));
  }, [displayed]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setSpace({ width, height });
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
      {displayed ? (
        <canvas
          ref={canvasRef}
          className={loading ? "viewer-canvas loading" : "viewer-canvas"}
          style={canvasStyle(space, shape ?? displayed.frame)}
        />
      ) : (
        !loading && <p className="viewer-empty">{placeholder}</p>
      )}
      {loading && (
        <span className="viewer-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          Loading…
        </span>
      )}
    </div>
  );
}

function canvasStyle(space: { width: number; height: number }, shape: { width: number; height: number }) {
  const size = fitSize(space, shape);
  // Before the first layout measurement, fall back to the stylesheet's max-size rules.
  return size.width > 0 ? { width: size.width, height: size.height } : undefined;
}
