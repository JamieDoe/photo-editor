/** Layout maths for the virtualised Library views (pure; see virtual.test.ts). */

export interface VisibleRange {
  /** First row to render. */
  first: number;
  /** One past the last row to render. */
  end: number;
}

/**
 * Rows of height `rowHeight` intersecting the viewport, plus `overscan` rows either
 * side so thumbnails just outside the view are already on their way. `listTop` is the
 * list's offset inside the scrolling element (content above it scrolls too).
 */
export function visibleRange(v: {
  scrollTop: number;
  viewportHeight: number;
  listTop: number;
  rowHeight: number;
  rowCount: number;
  overscan: number;
}): VisibleRange {
  if (v.rowCount <= 0 || v.rowHeight <= 0) return { first: 0, end: 0 };
  const top = v.scrollTop - v.listTop;
  const first = Math.max(0, Math.floor(top / v.rowHeight) - v.overscan);
  const end = Math.min(v.rowCount, Math.ceil((top + v.viewportHeight) / v.rowHeight) + v.overscan);
  return { first: Math.min(first, end), end };
}

export interface GridLayout {
  columns: number;
  cardWidth: number;
  /** Height of one grid row including the gap below it. */
  rowHeight: number;
}

export const GRID_GAP_X = 16;
export const GRID_GAP_Y = 20;
/** Card image (3:2) plus the caption line under it. */
const CAPTION_HEIGHT = 8 + 18;
/** Cards never grow wider than this, so 512 px thumbnails stay sharp on 2x screens. */
const MAX_CARD_WIDTH = 280;
const MIN_COLUMNS = 2;

/** Columns and row height for a grid `width` pixels wide. */
export function gridLayout(width: number): GridLayout {
  const usable = Math.max(width, 1);
  const columns = Math.max(MIN_COLUMNS, Math.ceil((usable + GRID_GAP_X) / (MAX_CARD_WIDTH + GRID_GAP_X)));
  const cardWidth = (usable - GRID_GAP_X * (columns - 1)) / columns;
  return { columns, cardWidth, rowHeight: Math.round((cardWidth * 2) / 3 + CAPTION_HEIGHT + GRID_GAP_Y) };
}

/**
 * The scroll position that brings row `rowIndex` fully into view, or null if it is
 * already visible. Used when the keyboard moves the selection.
 */
export function scrollToReveal(v: {
  scrollTop: number;
  viewportHeight: number;
  listTop: number;
  rowHeight: number;
  rowIndex: number;
}): number | null {
  const top = v.listTop + v.rowIndex * v.rowHeight;
  const bottom = top + v.rowHeight;
  if (top < v.scrollTop) return top;
  if (bottom > v.scrollTop + v.viewportHeight) return bottom - v.viewportHeight;
  return null;
}
