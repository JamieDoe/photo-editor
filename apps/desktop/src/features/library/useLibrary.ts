import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { CollectionKindDto } from "../../ipc/generated/CollectionKindDto";
import type { CollectionListingDto } from "../../ipc/generated/CollectionListingDto";
import type { FolderListingDto } from "../../ipc/generated/FolderListingDto";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import type { IndexEvent } from "../../ipc/generated/IndexEvent";
import type { LibraryStatusDto } from "../../ipc/generated/LibraryStatusDto";
import { applyChange, rangeToTick, stepFrom, visiblePhotos, type LibraryFilter } from "./marks";

export type IndexProgress = Extract<IndexEvent, { type: "progress" }>;
export type IndexFinished = Extract<IndexEvent, { type: "finished" }>;
export type LibraryLayout = "grid" | "list";

/**
 * Library state: browsing (folder listings) and the catalogue (indexing, totals).
 * Folder access is enforced in Rust: only folders chosen in the native dialog (and
 * remembered ones) can be listed or indexed.
 */
export function useLibrary() {
  const [listing, setListing] = useState<FolderListingDto | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [indexing, setIndexing] = useState<IndexProgress | null>(null);
  const [lastIndex, setLastIndex] = useState<IndexFinished | null>(null);
  const [status, setStatus] = useState<LibraryStatusDto | null>(null);
  // Kept here (not in the view) so they survive switching to Edit and back.
  const [layout, setLayout] = useState<LibraryLayout>("grid");
  const [filter, setFilter] = useState<LibraryFilter>("all");
  const [selected, setSelected] = useState<string | null>(null);
  /** Photos ticked for batch editing (ADR 0049), by path, in the order ticked; and
   *  where a ⇧-click range starts. */
  const [batch, setBatch] = useState<string[]>([]);
  const batchAnchor = useRef<string | null>(null);
  /** Bumped per photo when its edit changes, so its thumbnail is fetched again. */
  const [thumbRevs, setThumbRevs] = useState<Record<string, number>>({});
  /** A library-wide collection being viewed instead of a folder. */
  const [collection, setCollection] = useState<CollectionListingDto | null>(null);
  const requestRef = useRef(0);
  const listingRef = useRef<FolderListingDto | null>(null);
  listingRef.current = listing;

  const refreshStatus = useCallback(() => {
    ipc.libraryStatus().then(setStatus, () => undefined);
  }, []);

  /** Runs a listing request; responses superseded by a newer request are dropped. */
  const load = useCallback(async (request: () => Promise<FolderListingDto | null>) => {
    const id = ++requestRef.current;
    setLoading(true);
    try {
      const result = await request();
      if (id !== requestRef.current) return null;
      if (result) {
        setListing(result);
        setError(null);
      }
      return result;
    } catch (e) {
      if (id === requestRef.current) setError(await toAppError(e));
      return null;
    } finally {
      if (id === requestRef.current) setLoading(false);
    }
  }, []);

  const index = useCallback(async (path: string) => {
    try {
      await ipc.indexLibraryFolder(path);
    } catch (e) {
      setError(await toAppError(e));
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    const unlisten = ipc
      .onIndexEvent((e) => {
        if (e.type === "progress") {
          setIndexing(e);
          return;
        }
        setIndexing((current) => (current?.root === e.root ? null : current));
        if (e.type === "finished") {
          setLastIndex(e);
          refreshStatus();
          // The open folder may have gained or lost photos.
          const open = listingRef.current;
          if (open && open.breadcrumbs[0]?.path === e.root) void load(() => ipc.listFolder(open.path));
        } else {
          void toAppError(e.error).then(setError);
        }
      })
      .catch(() => () => {});
    return () => void unlisten.then((u) => u());
  }, [load, refreshStatus]);

  const chooseFolder = useCallback(async () => {
    const result = await load(ipc.chooseFolder);
    if (result) void index(result.path);
    return result;
  }, [load, index]);

  const openFolder = useCallback(
    async (path: string) => {
      const result = await load(() => ipc.listFolder(path));
      if (result) setCollection(null);
      return result;
    },
    [load],
  );

  const openCollection = useCallback(async (kind: CollectionKindDto) => {
    setLoading(true);
    requestRef.current++; // a folder listing still in flight must not replace this view
    try {
      setCollection(await ipc.libraryCollection(kind));
      setError(null);
    } catch (e) {
      setError(await toAppError(e));
    } finally {
      setLoading(false);
    }
  }, []);

  /** The photos of the current view (collection or folder), before filtering. */
  const photos: PhotoEntryDto[] = collection?.photos ?? listing?.photos ?? [];
  const photosRef = useRef(photos);
  photosRef.current = photos;
  /** What the grid shows: the view's photos after the filter (and collection membership). */
  const visible = useMemo(() => visiblePhotos(photos, filter, collection?.kind ?? null), [photos, filter, collection?.kind]);
  const visibleRef = useRef(visible);
  visibleRef.current = visible;

  const batchInView = useMemo(() => {
    const inView = new Set(photos.map((p) => p.path));
    return batch.filter((path) => inView.has(path));
  }, [batch, photos]);

  /** The photo `delta` places after `path` among the visible ones (Edit's ← →). */
  const neighbour = useCallback(
    (path: string | null, delta: number) => stepFrom(photosRef.current, visibleRef.current, path, delta),
    [],
  );

  /**
   * Rates or flags photos. Shown at once, then stored; the collection counts come back
   * from the catalogue. On failure the view is reloaded so it shows what was stored.
   */
  const setMarks = useCallback(
    async (paths: string[], change: MarkChangeDto) => {
      if (paths.length === 0) return;
      const targets = new Set(paths);
      const update = (ps: PhotoEntryDto[]) =>
        ps.map((p) => (targets.has(p.path) ? { ...p, marks: applyChange(p.marks, change) } : p));
      setListing((l) => (l ? { ...l, photos: update(l.photos) } : l));
      setCollection((c) => (c ? { ...c, photos: update(c.photos) } : c));
      try {
        const collections = await ipc.setPhotoMarks(paths, change);
        setStatus((s) => (s ? { ...s, collections } : s));
      } catch (e) {
        setError(await toAppError(e));
        const open = listingRef.current;
        if (open) void load(() => ipc.listFolder(open.path));
      }
    },
    [load],
  );

  /** A photo's edit was saved (or reset): update its "edited" state in the view, and
   *  fetch its thumbnail again where one is shown. */
  const markEdited = useCallback((path: string, edited: boolean) => {
    const update = (ps: PhotoEntryDto[]) => ps.map((p) => (p.path === path && p.edited !== edited ? { ...p, edited } : p));
    setListing((l) => (l ? { ...l, photos: update(l.photos) } : l));
    setCollection((c) => (c ? { ...c, photos: update(c.photos) } : c));
    setThumbRevs((r) => ({ ...r, [path]: (r[path] ?? 0) + 1 }));
  }, []);

  /** Ticks or unticks a photo for batch editing. */
  const toggleBatch = useCallback((path: string) => {
    batchAnchor.current = path;
    setBatch((b) => (b.includes(path) ? b.filter((p) => p !== path) : [...b, path]));
  }, []);
  /** Ticks every visible photo from the last one ticked to `path` (⇧-click). */
  const tickRange = useCallback((path: string) => {
    const range = rangeToTick(
      visibleRef.current.map((p) => p.path),
      batchAnchor.current,
      path,
    );
    if (range.length === 0) return;
    batchAnchor.current = path;
    setBatch((b) => [...b, ...range.filter((p) => !b.includes(p))]);
  }, []);
  const clearBatch = useCallback(() => {
    batchAnchor.current = null;
    setBatch([]);
  }, []);

  const findPhoto = useCallback((path: string | null) => (path ? (photosRef.current.find((p) => p.path === path) ?? null) : null), []);

  /** Re-lists the open folder and re-indexes its library root (fast when unchanged). */
  const refresh = useCallback(async () => {
    const open = listingRef.current;
    if (!open) return null;
    void index(open.path);
    return openFolder(open.path);
  }, [index, openFolder]);

  return {
    listing,
    loading,
    error,
    clearError: () => setError(null),
    indexing,
    lastIndex,
    status,
    layout,
    setLayout,
    filter,
    setFilter,
    selected,
    setSelected,
    collection,
    openCollection,
    photos,
    visible,
    neighbour,
    setMarks,
    markEdited,
    findPhoto,
    /** Batch selection (ADR 0049): the ticked photos still in this view. */
    batch: batchInView,
    toggleBatch,
    tickRange,
    clearBatch,
    thumbRevs,
    chooseFolder,
    openFolder,
    refresh,
  };
}

export type LibraryApi = ReturnType<typeof useLibrary>;
