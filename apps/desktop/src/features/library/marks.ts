/** Ratings, flags and filters: pure rules shared by the Library and Edit views. */
import type { CollectionKindDto } from "../../ipc/generated/CollectionKindDto";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { MarksDto } from "../../ipc/generated/MarksDto";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";

/** Keyboard shortcuts (as in other photo tools): 0–5 rate, P pick, X reject, U unflag. */
export function markChangeForKey(key: string): MarkChangeDto | null {
  if (/^[0-5]$/.test(key)) return { type: "rating", stars: Number(key) };
  switch (key.toLowerCase()) {
    case "p":
      return { type: "flag", flag: "pick" };
    case "x":
      return { type: "flag", flag: "reject" };
    case "u":
      return { type: "flag", flag: "none" };
    default:
      return null;
  }
}

/** Clicking the star that is already set clears the rating (as in the design). */
export function starClick(current: number, clicked: number): MarkChangeDto {
  return { type: "rating", stars: current === clicked ? 0 : clicked };
}

/** Clicking the active flag button removes the flag. */
export function flagClick(current: MarksDto["flag"], clicked: "pick" | "reject"): MarkChangeDto {
  return { type: "flag", flag: current === clicked ? "none" : clicked };
}

export function applyChange(marks: MarksDto, change: MarkChangeDto): MarksDto {
  return change.type === "rating" ? { ...marks, rating: change.stars } : { ...marks, flag: change.flag };
}

export type LibraryFilter = "all" | "picks" | "rated3";

export const FILTERS: ReadonlyArray<{ id: LibraryFilter; label: string }> = [
  { id: "all", label: "All" },
  { id: "picks", label: "Picks" },
  { id: "rated3", label: "★ 3+" },
];

export function matchesFilter(marks: MarksDto, filter: LibraryFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "picks":
      return marks.flag === "pick";
    case "rated3":
      return marks.rating >= 3;
  }
}

export function inCollection(marks: MarksDto, kind: CollectionKindDto): boolean {
  switch (kind) {
    case "picks":
      return marks.flag === "pick";
    case "rated":
      return marks.rating > 0;
    case "rejected":
      return marks.flag === "reject";
  }
}

export const COLLECTION_NAMES: Record<CollectionKindDto, string> = {
  picks: "Picks",
  rated: "Rated",
  rejected: "Rejected",
};

export const starsText = (rating: number) => "★".repeat(Math.max(0, Math.min(5, rating)));

/** Photos shown for a view: collection membership (marks can change while viewing), then the filter. */
export function visiblePhotos(
  photos: readonly PhotoEntryDto[],
  filter: LibraryFilter,
  collection: CollectionKindDto | null,
): PhotoEntryDto[] {
  return photos.filter((p) => (collection === null || inCollection(p.marks, collection)) && matchesFilter(p.marks, filter));
}

/**
 * The visible photo `delta` places from `path`. If `path` itself was just filtered out
 * (say, un-rejected while viewing Rejected), steps from where it was in `all`.
 */
export function stepFrom(
  all: readonly PhotoEntryDto[],
  visible: readonly PhotoEntryDto[],
  path: string | null,
  delta: number,
): PhotoEntryDto | null {
  if (!path || delta === 0) return null;
  const i = visible.findIndex((p) => p.path === path);
  if (i >= 0) return visible[i + delta] ?? null;
  const j = all.findIndex((p) => p.path === path);
  if (j < 0) return null;
  const shown = new Set(visible.map((p) => p.path));
  for (let k = j + delta; k >= 0 && k < all.length; k += Math.sign(delta)) {
    if (shown.has(all[k]!.path)) return all[k]!;
  }
  return null;
}
