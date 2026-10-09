/** Ratings, flags, colour labels, filters and sorting: pure rules shared by the Library
 *  and Edit views. */
import type { LibraryFilter } from "../../ipc/generated/LibraryFilter";
import type { LibrarySort } from "../../ipc/generated/LibrarySort";
import type { CollectionKindDto } from "../../ipc/generated/CollectionKindDto";
import type { ColourLabelDto } from "../../ipc/generated/ColourLabelDto";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { MarksDto } from "../../ipc/generated/MarksDto";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";

export type Label = Exclude<ColourLabelDto, "none">;

/** The colour labels (ADR 0064), with Lightroom's keys: 6–9 for the first four. */
export const LABELS: ReadonlyArray<{ id: Label; name: string; key: string | null }> = [
  { id: "red", name: "Red", key: "6" },
  { id: "yellow", name: "Yellow", key: "7" },
  { id: "green", name: "Green", key: "8" },
  { id: "blue", name: "Blue", key: "9" },
  { id: "purple", name: "Purple", key: null },
];

/** Choosing the label a photo already has removes it (as with stars and flags). */
export function labelClick(current: ColourLabelDto, clicked: Label): MarkChangeDto {
  return { type: "label", label: current === clicked ? "none" : clicked };
}

/**
 * Keyboard shortcuts (as in other photo tools): 0–5 rate, P pick, X reject, U unflag,
 * 6–9 red, yellow, green and blue (again to remove, judged by `current`).
 */
export function markChangeForKey(key: string, current?: MarksDto): MarkChangeDto | null {
  if (/^[0-5]$/.test(key)) return { type: "rating", stars: Number(key) };
  const label = LABELS.find((l) => l.key === key);
  if (label) return labelClick(current?.label ?? "none", label.id);
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
  switch (change.type) {
    case "rating":
      return { ...marks, rating: change.stars };
    case "flag":
      return { ...marks, flag: change.flag };
    case "label":
      return { ...marks, label: change.label };
  }
}

export type { LibraryFilter } from "../../ipc/generated/LibraryFilter";

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
    case "all":
      return true;
    case "picks":
      return marks.flag === "pick";
    case "rated":
      return marks.rating > 0;
    case "rejected":
      return marks.flag === "reject";
    case "favourites":
      // Five stars (ADR 0081).
      return marks.rating === 5;
    case "recent":
    case "edited":
      // Not a mark: the catalogue chose them.
      return true;
  }
}

export const COLLECTION_NAMES: Record<CollectionKindDto, string> = {
  all: "All photos",
  picks: "Picks",
  rated: "Rated",
  rejected: "Rejected",
  favourites: "Favourites",
  recent: "Recently imported",
  edited: "Recently edited",
};

/** Whether a collection keeps the catalogue's order instead of the chosen sort:
 *  Recently edited lists the latest edits first (ADR 0079). */
export const keepsOwnOrder = (kind: CollectionKindDto | null) => kind === "edited";

export const starsText = (rating: number) => "★".repeat(Math.max(0, Math.min(5, rating)));

/**
 * Photos shown for a view: collection membership (marks can change while viewing), then
 * the filter and the colour label chosen, if any.
 */
export function visiblePhotos(
  photos: readonly PhotoEntryDto[],
  filter: LibraryFilter,
  collection: CollectionKindDto | null,
  label: Label | null = null,
): PhotoEntryDto[] {
  return photos.filter(
    (p) =>
      (collection === null || inCollection(p.marks, collection)) &&
      matchesFilter(p.marks, filter) &&
      (label === null || p.marks.label === label),
  );
}

/** The Library's orders (ADR 0064). Capture time first, as in other photo tools. */
export type { LibrarySort } from "../../ipc/generated/LibrarySort";

export const SORTS: ReadonlyArray<{ id: LibrarySort; label: string }> = [
  { id: "captured", label: "Capture time" },
  { id: "newest", label: "Newest first" },
  { id: "name", label: "File name" },
  { id: "rating", label: "Rating" },
];

/** File names as people read them: DSC_9 before DSC_10, case ignored. */
const names = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/**
 * `photos` in `sort` order. Photos whose capture time is not known yet (not indexed)
 * come after the rest, by name; ties go to the name too, so the order is stable.
 */
export function sortPhotos(photos: readonly PhotoEntryDto[], sort: LibrarySort): PhotoEntryDto[] {
  const byName = (a: PhotoEntryDto, b: PhotoEntryDto) => names.compare(a.name, b.name) || (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  const byTime = (a: PhotoEntryDto, b: PhotoEntryDto, newest: boolean) => {
    const x = a.details?.capturedAt ?? null;
    const y = b.details?.capturedAt ?? null;
    if (x === y) return byName(a, b);
    if (x === null) return 1;
    if (y === null) return -1;
    return (x < y ? -1 : 1) * (newest ? -1 : 1);
  };
  const compare: (a: PhotoEntryDto, b: PhotoEntryDto) => number =
    sort === "name"
      ? byName
      : sort === "rating"
        ? (a, b) => b.marks.rating - a.marks.rating || byTime(a, b, false)
        : (a, b) => byTime(a, b, sort === "newest");
  return [...photos].sort(compare);
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

/**
 * The photos a ⇧-click ticks (ADR 0049): every shown photo from `anchor` (the last one
 * ticked) to `path`, either way round; just `path` without an anchor on show.
 */
export function rangeToTick(shown: readonly string[], anchor: string | null, path: string): string[] {
  const to = shown.indexOf(path);
  if (to < 0) return [];
  const from = anchor === null ? -1 : shown.indexOf(anchor);
  return from < 0 ? [path] : shown.slice(Math.min(from, to), Math.max(from, to) + 1);
}
