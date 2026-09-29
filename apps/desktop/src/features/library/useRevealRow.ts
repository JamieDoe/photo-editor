import { useEffect, type RefObject } from "react";
import { scrollToReveal } from "./virtual";

/** Scrolls `scrollRef` so row `rowIndex` of the list in `listRef` is visible. */
export function useRevealRow(
  scrollRef: RefObject<HTMLElement | null>,
  listRef: RefObject<HTMLElement | null>,
  rowIndex: number | null,
  rowHeight: number,
) {
  useEffect(() => {
    const scroller = scrollRef.current;
    if (rowIndex === null || rowIndex < 0 || !scroller || rowHeight <= 0) return;
    const target = scrollToReveal({
      scrollTop: scroller.scrollTop,
      viewportHeight: scroller.clientHeight,
      listTop: listRef.current?.offsetTop ?? 0,
      rowHeight,
      rowIndex,
    });
    if (target !== null) scroller.scrollTop = target;
  }, [scrollRef, listRef, rowIndex, rowHeight]);
}
