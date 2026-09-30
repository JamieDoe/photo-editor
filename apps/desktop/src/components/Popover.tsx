import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

const WIDTH = 272;
/** Space kept between the popover and the window's edges, and its anchor. */
const MARGIN = 8;

/**
 * A small floating panel under `anchor`, drawn over everything (so a scrolling strip
 * does not clip it). Follows the anchor when something scrolls or the window resizes;
 * closes on a click elsewhere, Escape, or when the anchor goes away.
 */
export function Popover({
  anchor,
  label,
  onClose,
  children,
}: {
  anchor: HTMLElement;
  label: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);

  /** Right edges aligned, inside the window; above the anchor if there is no room below. */
  const place = useCallback(() => {
    const a = anchor.getBoundingClientRect();
    const height = ref.current?.offsetHeight ?? 0;
    const left = Math.min(Math.max(MARGIN, a.right - WIDTH), window.innerWidth - WIDTH - MARGIN);
    const below = a.bottom + MARGIN;
    const top = below + height > window.innerHeight - MARGIN ? Math.max(MARGIN, a.top - MARGIN - height) : below;
    setAt({ left, top });
  }, [anchor]);
  useLayoutEffect(place, [place]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!ref.current?.contains(t) && !anchor.contains(t)) onClose();
    };
    // After the popover's own controls, which may use Escape themselves (and stop it).
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onMove = () => {
      if (anchor.isConnected) place();
      else onClose();
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onMove, true);
    window.addEventListener("resize", onMove);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onMove, true);
      window.removeEventListener("resize", onMove);
    };
  }, [anchor, onClose, place]);

  return createPortal(
    <div
      ref={ref}
      className="popover"
      role="dialog"
      aria-label={label}
      style={{ width: WIDTH, left: at?.left ?? 0, top: at?.top ?? 0, visibility: at ? "visible" : "hidden" }}
    >
      {children}
    </div>,
    document.body,
  );
}

/** A popover's header, as the design's dialogs have it: a small picture (a thumbnail,
 *  or an icon), a title (or a field in its place) and a line under it. */
export function PopoverHeader({ visual, title, sub }: { visual: ReactNode; title: ReactNode; sub: string }) {
  return (
    <div className="popover-header">
      {visual}
      <div className="popover-heading">
        {title}
        <span className="popover-sub">{sub}</span>
      </div>
    </div>
  );
}

/** An icon in the header's picture spot. */
export function PopoverIcon({ children }: { children: ReactNode }) {
  return <span className="popover-thumb popover-icon">{children}</span>;
}
