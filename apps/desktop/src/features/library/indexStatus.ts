import type { IndexFinished, IndexProgress } from "./useLibrary";

/** One-line indexing status for the Library toolbar, or null when there is nothing to say. */
export function indexStatusText(indexing: IndexProgress | null, last: IndexFinished | null): string | null {
  if (indexing) {
    return indexing.processed === 0
      ? `Indexing… found ${indexing.found.toLocaleString()} photos`
      : `Indexing ${indexing.processed.toLocaleString()} of ${indexing.found.toLocaleString()}…`;
  }
  if (!last) return null;
  const parts = [`${last.found.toLocaleString()} photo${last.found === 1 ? "" : "s"} indexed`];
  if (last.new > 0) parts.push(`${last.new.toLocaleString()} new`);
  if (last.moved > 0) parts.push(`${last.moved.toLocaleString()} moved`);
  if (last.changed > 0) parts.push(`${last.changed.toLocaleString()} changed`);
  if (last.missing > 0) parts.push(`${last.missing.toLocaleString()} missing`);
  if (last.skipped > 0) parts.push(`${last.skipped.toLocaleString()} unreadable`);
  return parts.join(" · ");
}
