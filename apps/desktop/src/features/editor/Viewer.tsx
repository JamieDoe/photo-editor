import { type PointerEvent as ReactPointerEvent, type ReactNode, type WheelEvent as ReactWheelEvent, useEffect, useRef, useState } from "react";
import type { DisplayedFrame } from "./useEditor";
import { fitSize, nextBoxShape, type BoxShape } from "./viewerLayout";
import { isTextEntry } from "../../lib/keyboard";
import { ViewerZoomContext } from "./viewerZoom";
import { centreKeeping, panned, visiblePart, zoomLayout, type OutputWindow, type ZoomCentre } from "./zoom";

interface Props {
  displayed: DisplayedFrame | null;
  /** Another photo is opening: the current one stays, dimmed, until it arrives. */
  loading: boolean;
  /** Reports the viewport's long edge in device pixels. */
  onResize: (longEdgeDevicePx: number) => void;
  placeholder: string;
  /** Drawn over the photo, in its box (the crop tool). */
  overlay?: ReactNode;
  /** Zoom (ADR 0070): where the photo is centred at 100 %, or null at Fit. Without
   *  `onZoom` the photo can't be zoomed (a tool is open). With an overlay (retouch),
   *  clicks are the overlay's: the photo pans by scrolling or Space-dragging. */
  zoom?: ZoomCentre | null;
  onZoom?: (centre: ZoomCentre | null) => void;
  /** At 100 %, the latest render of the visible part, drawn over the whole photo. */
  windowed?: DisplayedFrame | null;
  /** Reports the part of the photo shown at 100 % (null at Fit), for rendering. */
  onWindow?: (window: OutputWindow | null) => void;
}

/** A pointer moving less than this (CSS px) between down and up is a click. */
const CLICK_SLOP = 4;

/**
 * Blits Rust-rendered RGBA frames to a canvas. No pixel processing happens here. The
 * canvas is sized to fit the viewer (not to the frame's pixel count), so a quick
 * low-resolution frame and the later sharp one appear at the same size.
 */
export function Viewer({ displayed, loading, onResize, placeholder, overlay, zoom = null, onZoom, windowed = null, onWindow }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const windowCanvasRef = useRef<HTMLCanvasElement>(null);
  const zoomRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ id: number; x: number; y: number; centre: ZoomCentre; moved: boolean } | null>(null);
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

  // At 100 %: the photo's box at full resolution, positioned by the zoom centre.
  const dpr = window.devicePixelRatio || 1;
  const full = displayed && displayed.frame.fullWidth > 0 ? { width: displayed.frame.fullWidth, height: displayed.frame.fullHeight } : null;
  const zoomed = zoom !== null && onZoom !== undefined && full !== null;
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
  const visibleKey = visible?.join(",") ?? "";
  // Reported by value (the array is new on every render).
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  useEffect(() => onWindow?.(visibleRef.current), [visibleKey, onWindow]);

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

  /** Zooms to 100 % keeping the photo's point under `client` (a pointer) in place. */
  const zoomInAt = (client: { x: number; y: number }) => {
    const container = containerRef.current;
    const photo = container?.querySelector(".viewer-frame, .viewer-canvas")?.getBoundingClientRect();
    if (!onZoom || !full || !container || !photo || photo.width === 0) return;
    const at = {
      x: Math.min(1, Math.max(0, (client.x - photo.left) / photo.width)),
      y: Math.min(1, Math.max(0, (client.y - photo.top) / photo.height)),
    };
    const box = container.getBoundingClientRect();
    const style = getComputedStyle(container);
    const pointer = { x: client.x - box.left - parseFloat(style.paddingLeft), y: client.y - box.top - parseFloat(style.paddingTop) };
    onZoom(centreKeeping(space, full, dpr, at, pointer));
  };
  /** Fit: a click zooms to 100 % on the spot clicked. */
  const zoomIn = (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button === 0) zoomInAt({ x: e.clientX, y: e.clientY });
  };

  // Pinching on a trackpad: apart zooms to 100 % where the fingers are, together fits.
  // WebKit reports a pinch as gesture events, other engines as a wheel with Ctrl; the
  // page itself never zooms.
  const pinchRef = useRef({ zoomIn: zoomInAt, zoomed, fit: () => onZoom?.(null), allowed: onZoom !== undefined && full !== null, spread: 0 });
  pinchRef.current = { ...pinchRef.current, zoomIn: zoomInAt, zoomed, fit: () => onZoom?.(null), allowed: onZoom !== undefined && full !== null };
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const p = pinchRef;
    /** A pinch's accumulated spread (log of its scale): ±0.15 decides it. */
    const settle = (client: { x: number; y: number }) => {
      const { spread } = p.current;
      if (spread > 0.15 && !p.current.zoomed) {
        p.current.zoomIn(client);
        p.current.spread = 0;
      } else if (spread < -0.15 && p.current.zoomed) {
        p.current.fit();
        p.current.spread = 0;
      }
    };
    let lastWheel = 0;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      if (!p.current.allowed) return;
      // A pause ends one pinch; the next starts afresh.
      if (e.timeStamp - lastWheel > 300) p.current.spread = 0;
      lastWheel = e.timeStamp;
      p.current.spread -= e.deltaY / 100;
      settle({ x: e.clientX, y: e.clientY });
    };
    let last = 1;
    type Gesture = Event & { scale: number; clientX: number; clientY: number };
    const onGestureStart = (e: Event) => {
      e.preventDefault();
      last = 1;
      p.current.spread = 0;
    };
    const onGestureChange = (e: Event) => {
      e.preventDefault();
      const g = e as Gesture;
      if (!p.current.allowed || !(g.scale > 0)) return;
      p.current.spread += Math.log(g.scale / last);
      last = g.scale;
      settle({ x: g.clientX, y: g.clientY });
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
    dragRef.current = { id: e.pointerId, x: e.clientX, y: e.clientY, centre: zoom, moved: false };
  };
  const zoomMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.id !== e.pointerId || !full || !onZoom) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < CLICK_SLOP) return;
    if (!drag.moved) setPanning(true);
    drag.moved = true;
    onZoom(panned(space, full, dpr, drag.centre, dx, dy));
  };
  const zoomUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.id !== e.pointerId) return;
    dragRef.current = null;
    setPanning(false);
    // A click (no drag) goes back to Fit, unless clicks are the overlay's.
    if (!drag.moved && e.type === "pointerup" && overlay === undefined) onZoom?.(null);
  };
  /** Scrolling (two fingers on a trackpad) pans. */
  const zoomWheel = (e: ReactWheelEvent<HTMLDivElement>) => {
    if (!zoom || !full || !onZoom || e.ctrlKey) return;
    const lines = e.deltaMode === 1 ? 16 : 1;
    onZoom(panned(space, full, dpr, zoom, -e.deltaX * lines, -e.deltaY * lines));
  };

  return (
    <div className="viewer" ref={containerRef}>
      {displayed ? (
        layout && shape ? (
          <div
            ref={zoomRef}
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
                    left: shownWindow.frame.window.x / dpr,
                    top: shownWindow.frame.window.y / dpr,
                    width: shownWindow.frame.window.width / dpr,
                    height: shownWindow.frame.window.height / dpr,
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
            className={["viewer-canvas", loading && "loading", onZoom && "zoomable"].filter(Boolean).join(" ")}
            style={canvasStyle(space, shape ?? displayed.frame)}
            onPointerUp={onZoom ? zoomIn : undefined}
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
