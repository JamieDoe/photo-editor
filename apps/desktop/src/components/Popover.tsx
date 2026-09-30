import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

const WIDTH = 248;
/** Space kept between the popover and the window's edges, and its anchor. */
const MARGIN = 8;

/**
 * A small floating panel under `anchor`, drawn over everything (so a scrolling strip
 * does not clip it). Closes on a click elsewhere, Escape, or when the page scrolls or
 * resizes (the anchor would move away from it).
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

  useLayoutEffect(() => {
    const a = anchor.getBoundingClientRect();
    const height = ref.current?.offsetHeight ?? 0;
    // Right edges aligned, inside the window; above the anchor if there is no room below.
    const left = Math.min(Math.max(MARGIN, a.right - WIDTH), window.innerWidth - WIDTH - MARGIN);
    const below = a.bottom + MARGIN;
    const top = below + height > window.innerHeight - MARGIN ? Math.max(MARGIN, a.top - MARGIN - height) : below;
    setAt({ left, top });
  }, [anchor]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!ref.current?.contains(t) && !anchor.contains(t)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    const onMove = (e: Event) => {
      if (!(e.target instanceof Node && ref.current?.contains(e.target))) onClose();
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("scroll", onMove, true);
    window.addEventListener("resize", onMove);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("scroll", onMove, true);
      window.removeEventListener("resize", onMove);
    };
  }, [anchor, onClose]);

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
