import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { visibleRange, type VisibleRange } from "./virtual";

/**
 * The rows of a list inside `scrollRef` that should be rendered, recomputed on scroll
 * and resize. Re-renders only when the range changes, not on every scroll event.
 */
export function useVirtualRows(
  scrollRef: RefObject<HTMLElement | null>,
  listRef: RefObject<HTMLElement | null>,
  rowHeight: number,
  rowCount: number,
  overscan = 2,
): VisibleRange {
  const [range, setRange] = useState<VisibleRange>({ first: 0, end: Math.min(rowCount, 12) });
  const params = useRef({ rowHeight, rowCount, overscan });
  params.current = { rowHeight, rowCount, overscan };
  const update = useRef(() => {});

  // A passive effect: the scroller usually belongs to a parent, whose ref is attached
  // only after this component's layout effects have run.
  useEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    update.current = () => {
      const next = visibleRange({
        scrollTop: scroller.scrollTop,
        viewportHeight: scroller.clientHeight,
        listTop: listRef.current?.offsetTop ?? 0,
        ...params.current,
      });
      setRange((r) => (r.first === next.first && r.end === next.end ? r : next));
    };
    const onChange = () => update.current();
    onChange();
    scroller.addEventListener("scroll", onChange, { passive: true });
    const observer = new ResizeObserver(onChange);
    observer.observe(scroller);
    return () => {
      scroller.removeEventListener("scroll", onChange);
      observer.disconnect();
    };
  }, [scrollRef, listRef]);

  // Row size or count changed (resize, new listing): recompute with the new values.
  useLayoutEffect(() => update.current(), [rowHeight, rowCount, overscan]);
  return range;
}

/** Width of an element, tracked with a ResizeObserver. */
export function useWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(entry.contentRect.width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}
