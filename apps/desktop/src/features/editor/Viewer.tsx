import { type PointerEvent as ReactPointerEvent, type ReactNode, type WheelEvent as ReactWheelEvent, useEffect, useRef, useState } from "react";
import { isTextEntry } from "../../lib/keyboard";
import type { DisplayedFrame } from "./useEditor";
import type { ViewPoint, ZoomControl } from "./useZoom";
import { fitSize, nextBoxShape, type BoxShape } from "./viewerLayout";
import { ViewerZoomContext } from "./viewerZoom";
import { visiblePart, zoomLayout, type OutputWindow, type Zoom } from "./zoom";

interface Props {
  displayed: DisplayedFrame | null;
  /** Another photo is opening: the current one stays, dimmed, until it arrives. */
  loading: boolean;
  /** Reports the viewport's long edge in device pixels. */
  onResize: (longEdgeDevicePx: number) => void;
  placeholder: string;
  /** Drawn over the photo, in its box (the crop tool). */
  overlay?: ReactNode;
  /** Zoom (ADR 0070), or null at Fit. Without `zoomControl` the photo can't be zoomed
   *  (a tool is open). With an overlay (retouch, masks), clicks are the overlay's: the
   *  photo pans by scrolling or Space-dragging. */
  zoom?: Zoom | null;
  zoomControl?: ZoomControl;
  /** Zoomed, the latest render of the visible part, drawn over the whole photo. */
  windowed?: DisplayedFrame | null;
  /** Reports the part of the photo in view when zoomed (null at Fit), in
   *  full-resolution pixels, and the long edge the whole photo has at the zoom. */
  onWindow?: (window: OutputWindow | null, zoomedLongEdge: number) => void;
}

/** A pointer moving less than this (CSS px) between down and up is a click. */
const CLICK_SLOP = 4;

/**
 * Blits Rust-rendered RGBA frames to a canvas. No pixel processing happens here. The
 * canvas is sized to fit the viewer (not to the frame's pixel count), so a quick
 * low-resolution frame and the later sharp one appear at the same size.
 */
export function Viewer({ displayed, loading, onResize, placeholder, overlay, zoom = null, zoomControl, windowed = null, onWindow }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const windowCanvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<{ id: number; x: number; y: number; moved: boolean } | null>(null);
  const [panning, setPanning] = useState(false);
  // Space held: drags pan even over an overlay, as in other editors.
  const [spaceHeld, setSpaceHeld] = useState(false);
  const [space, setSpace] = useState({ width: 0, height: 0 });
  // The box shape is fixed per opening (see nextBoxShape), so frames never resize it.
  const [shape, setShape] = useState<BoxShape | null>(null);
  useEffect(() => {
    if (!displayed) return;
    const { frame } = displayed;
    // The frame's full-resolution output size (after crop) is its exact shape.
    const full = frame.fullWidth > 0 ? { width: frame.fullWidth, height: frame.fullHeight } : null;
    setShape((prev) => nextBoxShape(prev, displayed.imageId, frame, full));
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

  // Zoomed: the photo's box at the zoom's scale, positioned by its centre.
  const dpr = window.devicePixelRatio || 1;
  const full = displayed && displayed.frame.fullWidth > 0 ? { width: displayed.frame.fullWidth, height: displayed.frame.fullHeight } : null;
  const [fullWidth, fullHeight] = [full?.width ?? 0, full?.height ?? 0];
  useEffect(() => {
    zoomControl?.setGeometry(fullWidth > 0 && space.width > 0 ? { view: space, full: { width: fullWidth, height: fullHeight }, dpr } : null);
  }, [zoomControl, space, fullWidth, fullHeight, dpr]);
  const zoomed = zoom !== null && zoomControl !== undefined && full !== null;
  useEffect(() => {
    if (!zoomed || overlay === undefined) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== " " || isTextEntry(e.target) || e.target instanceof HTMLInputElement) return;
      e.preventDefault();
      setSpaceHeld(e.type === "keydown");
    };
    const release = () => setSpaceHeld(false);
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
      window.removeEventListener("blur", release);
      setSpaceHeld(false);
    };
  }, [zoomed, overlay === undefined]);
  const layout = zoomed && space.width > 0 ? zoomLayout(space, full, dpr, zoom) : null;
  const visible = layout?.window ?? null;
  // What overlays need of it: the part in view and how much larger than at Fit.
  const fitWidth = shape ? fitSize(space, shape).width : 0;
  const zoomInfo = layout && fitWidth > 0 ? { visible: visiblePart(space, layout), magnification: layout.width / fitWidth } : null;
  // Rendering asks for the whole photo's long edge at the zoom, which picks its source.
  const zoomedLongEdge = zoomed ? Math.round(Math.max(full.width, full.height) * zoom.scale) : 0;
  const visibleKey = `${visible?.join(",") ?? ""}@${zoomedLongEdge}`;
  // Reported by value (the array is new on every render).
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  useEffect(() => onWindow?.(visibleRef.current, zoomedLongEdge), [visibleKey, onWindow]);

  // With an overlay the canvas sits in a frame, without one it stands alone (and
  // zoomed, in the zoomed box): a different element, so the frame is drawn again when
  // that changes (leaving mask mode renders nothing new, and would otherwise show an
  // empty canvas).
  const framed = overlay !== undefined;
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
  }, [displayed, framed, zoomed]);

  // The window's render is drawn over the whole photo where it belongs.
  const shownWindow = zoomed && windowed && windowed.imageId === displayed?.imageId && windowed.frame.fullWidth === full?.width && windowed.frame.fullHeight === full?.height ? windowed : null;
  useEffect(() => {
    const canvas = windowCanvasRef.current;
    if (!canvas || !shownWindow) return;
    const { frame } = shownWindow;
    if (canvas.width !== frame.width || canvas.height !== frame.height) {
      canvas.width = frame.width;
      canvas.height = frame.height;
    }
    canvas.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  }, [shownWindow]);

  /** A pointer's place in the viewport (the viewer's content box), in CSS pixels. */
  const toView = (clientX: number, clientY: number): ViewPoint => {
    const container = containerRef.current;
    if (!container) return { x: 0, y: 0 };
    const box = container.getBoundingClientRect();
    const style = getComputedStyle(container);
    return { x: clientX - box.left - parseFloat(style.paddingLeft), y: clientY - box.top - parseFloat(style.paddingTop) };
  };
  const toViewRef = useRef(toView);
  toViewRef.current = toView;
  /** Fit: a click zooms to 100 % on the spot clicked. */
  const zoomIn = (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button === 0) zoomControl?.zoomIn(toView(e.clientX, e.clientY));
  };

  // Pinching on a trackpad zooms smoothly around the fingers, down to Fit. WebKit
  // reports a pinch as gesture events, other engines as a wheel with Ctrl; the page
  // itself never zooms.
  const controlRef = useRef(zoomControl);
  controlRef.current = zoomControl;
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const lines = e.deltaMode === 1 ? 16 : 1;
      controlRef.current?.zoomBy(Math.exp((-e.deltaY * lines) / 100), toViewRef.current(e.clientX, e.clientY));
    };
    // Safari's gesture scale is since the gesture began; each change applies its step.
    let last = 1;
    type Gesture = Event & { scale: number; clientX: number; clientY: number };
    const onGestureStart = (e: Event) => {
      e.preventDefault();
      last = 1;
    };
    const onGestureChange = (e: Event) => {
      e.preventDefault();
      const g = e as Gesture;
      if (!(g.scale > 0)) return;
      controlRef.current?.zoomBy(g.scale / last, toViewRef.current(g.clientX, g.clientY));
      last = g.scale;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    el.addEventListener("gesturestart", onGestureStart);
    el.addEventListener("gesturechange", onGestureChange);
    el.addEventListener("gestureend", onGestureStart);
    return () => {
      el.removeEventListener("wheel", onWheel);
      el.removeEventListener("gesturestart", onGestureStart);
      el.removeEventListener("gesturechange", onGestureChange);
      el.removeEventListener("gestureend", onGestureStart);
    };
  }, []);
  const zoomDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!zoom || e.button !== 0) return;
    // Over an overlay, a drag is the overlay's unless Space is held.
    if (overlay !== undefined && !spaceHeld) return;
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { id: e.pointerId, x: e.clientX, y: e.clientY, moved: false };
  };
  const zoomMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.id !== e.pointerId) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < CLICK_SLOP) return;
    if (!drag.moved) setPanning(true);
    drag.moved = true;
    [drag.x, drag.y] = [e.clientX, e.clientY];
    zoomControl?.panBy(dx, dy);
  };
  const zoomUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.id !== e.pointerId) return;
    dragRef.current = null;
    setPanning(false);
    // A click (no drag) goes back to Fit, unless clicks are the overlay's.
    if (!drag.moved && e.type === "pointerup" && overlay === undefined) zoomControl?.fit();
  };
  /** Scrolling (two fingers on a trackpad) pans. */
  const zoomWheel = (e: ReactWheelEvent<HTMLDivElement>) => {
    if (!zoom || e.ctrlKey) return;
    const lines = e.deltaMode === 1 ? 16 : 1;
    zoomControl?.panBy(-e.deltaX * lines, -e.deltaY * lines);
  };
  // Output pixels to the box's CSS pixels.
  const perPixel = zoomed ? zoom.scale / dpr : 1;

  return (
    <div className="viewer" ref={containerRef}>
      {displayed ? (
        layout && shape ? (
          <div
            className={["viewer-zoom", panning && "panning", overlay !== undefined && !spaceHeld && "tool"].filter(Boolean).join(" ")}
            onPointerDownCapture={zoomDown}
            onPointerMove={zoomMove}
            onPointerUp={zoomUp}
            onPointerCancel={zoomUp}
            onWheel={zoomWheel}
          >
            <div className="viewer-zoom-box" style={{ left: layout.left, top: layout.top, width: layout.width, height: layout.height }}>
              <canvas ref={canvasRef} className="viewer-canvas fill" />
              {shownWindow?.frame.window && (
                <canvas
                  ref={windowCanvasRef}
                  className="viewer-window"
                  style={{
                    left: shownWindow.frame.window.x * perPixel,
                    top: shownWindow.frame.window.y * perPixel,
                    width: shownWindow.frame.window.width * perPixel,
                    height: shownWindow.frame.window.height * perPixel,
                  }}
                />
              )}
              {zoomInfo && <ViewerZoomContext.Provider value={zoomInfo}>{overlay}</ViewerZoomContext.Provider>}
            </div>
          </div>
        ) : overlay ? (
          <div className="viewer-frame" style={canvasStyle(space, shape ?? displayed.frame)}>
            <canvas ref={canvasRef} className="viewer-canvas fill" />
            {overlay}
          </div>
        ) : (
          <canvas
            ref={canvasRef}
            className={["viewer-canvas", loading && "loading", zoomControl && "zoomable"].filter(Boolean).join(" ")}
            style={canvasStyle(space, shape ?? displayed.frame)}
            onPointerUp={zoomControl ? zoomIn : undefined}
          />
        )
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
