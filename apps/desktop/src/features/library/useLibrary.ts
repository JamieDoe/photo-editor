import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { AlbumDto } from "../../ipc/generated/AlbumDto";
import type { AlbumListingDto } from "../../ipc/generated/AlbumListingDto";
import type { CollectionKindDto } from "../../ipc/generated/CollectionKindDto";
import type { CollectionListingDto } from "../../ipc/generated/CollectionListingDto";
import type { FolderListingDto } from "../../ipc/generated/FolderListingDto";
import type { MarkChangeDto } from "../../ipc/generated/MarkChangeDto";
import type { SearchResultsDto } from "../../ipc/generated/SearchResultsDto";
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
  /** The albums (ADR 0055), and the one being viewed instead of a folder. */
  const [albums, setAlbums] = useState<AlbumDto[]>([]);
  const [album, setAlbum] = useState<AlbumListingDto | null>(null);
  /** A search of the whole library (ADR 0056): what is typed, and its results, shown
   *  over whatever view is underneath until cleared. */
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState<SearchResultsDto | null>(null);
  const searchSeq = useRef(0);
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

  const refreshAlbums = useCallback(() => {
    ipc.listAlbums().then(setAlbums, () => undefined);
  }, []);

  useEffect(() => {
    refreshStatus();
    refreshAlbums();
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
          refreshAlbums();
          // The open folder may have gained or lost photos.
          const open = listingRef.current;
          if (open && open.breadcrumbs[0]?.path === e.root) void load(() => ipc.listFolder(open.path));
        } else {
          void toAppError(e.error).then(setError);
        }
      })
      .catch(() => () => {});
    return () => void unlisten.then((u) => u());
  }, [load, refreshStatus, refreshAlbums]);

  const chooseFolder = useCallback(async () => {
    const result = await load(ipc.chooseFolder);
    if (result) void index(result.path);
    return result;
  }, [load, index]);

  /** Searches as the photographer types: after a short pause, the newest query only. */
  const runSearch = useCallback((q: string) => {
    setQuery(q);
    const id = ++searchSeq.current;
    if (!q.trim()) {
      setSearch(null);
      return;
    }
    window.setTimeout(() => {
      if (id !== searchSeq.current) return;
      ipc.searchLibrary(q).then(
        (results) => id === searchSeq.current && setSearch(results),
        async (e: unknown) => id === searchSeq.current && setError(await toAppError(e)),
      );
    }, 180);
  }, []);
  const clearSearch = useCallback(() => {
    searchSeq.current++;
    setQuery("");
    setSearch(null);
  }, []);

  const openFolder = useCallback(
    async (path: string) => {
      const result = await load(() => ipc.listFolder(path));
      if (result) {
        setCollection(null);
        setAlbum(null);
        clearSearch();
      }
      return result;
    },
    [load, clearSearch],
  );

  const openCollection = useCallback(async (kind: CollectionKindDto) => {
    setLoading(true);
    requestRef.current++; // a folder listing still in flight must not replace this view
    try {
      setCollection(await ipc.libraryCollection(kind));
      setAlbum(null);
      clearSearch();
      setError(null);
    } catch (e) {
      setError(await toAppError(e));
    } finally {
      setLoading(false);
    }
  }, [clearSearch]);

  const openAlbum = useCallback(async (id: number) => {
    setLoading(true);
    requestRef.current++; // a folder listing still in flight must not replace this view
    try {
      const listing = await ipc.albumPhotos(id);
      setAlbum(listing);
      setCollection(null);
      clearSearch();
      setAlbums((all) => all.map((a) => (a.id === id ? listing.album : a)));
      setError(null);
    } catch (e) {
      setError(await toAppError(e));
      refreshAlbums();
    } finally {
      setLoading(false);
    }
  }, [refreshAlbums, clearSearch]);

  /** Runs an album change; the albums (and the album shown, if it is the one changed)
   *  are refreshed after. Errors are shown; returns the album, or null. */
  const albumChange = useCallback(
    async (change: () => Promise<AlbumDto>): Promise<AlbumDto | null> => {
      try {
        const changed = await change();
        refreshAlbums();
        setAlbum((shown) => shown && shown.album.id === changed.id ? { ...shown, album: changed } : shown);
        return changed;
      } catch (e) {
        setError(await toAppError(e));
        refreshAlbums();
        return null;
      }
    },
    [refreshAlbums],
  );

  /** The photos of the current view (search, album, collection or folder), before
   *  filtering. */
  const photos: PhotoEntryDto[] = search?.photos ?? album?.photos ?? collection?.photos ?? listing?.photos ?? [];
  const shownCollection = search || album ? null : (collection?.kind ?? null);
  const photosRef = useRef(photos);
  photosRef.current = photos;
  /** What the grid shows: the view's photos after the filter (and collection membership). */
  const visible = useMemo(() => visiblePhotos(photos, filter, shownCollection), [photos, filter, shownCollection]);
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
      setAlbum((a) => (a ? { ...a, photos: update(a.photos) } : a));
      setSearch((s) => (s ? { ...s, photos: update(s.photos) } : s));
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
    setAlbum((a) => (a ? { ...a, photos: update(a.photos) } : a));
    setSearch((s) => (s ? { ...s, photos: update(s.photos) } : s));
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
    albums,
    album,
    openAlbum,
    /** The search box's text, and the results while it has any. */
    query,
    search,
    runSearch,
    clearSearch,
    /** A new album holding `paths`; opened when `open` is set. */
    createAlbum: async (name: string, paths: string[], open = false) => {
      const made = await albumChange(() => ipc.createAlbum(name, paths));
      if (made && open) void openAlbum(made.id);
      return made;
    },
    renameAlbum: (id: number, name: string) => albumChange(() => ipc.renameAlbum(id, name)),
    deleteAlbum: async (id: number) => {
      try {
        await ipc.deleteAlbum(id);
        setAlbum((shown) => (shown?.album.id === id ? null : shown));
      } catch (e) {
        setError(await toAppError(e));
      }
      refreshAlbums();
    },
    addToAlbum: (id: number, paths: string[]) => albumChange(() => ipc.addToAlbum(id, paths)),
    /** Takes `paths` out of an album; out of the view too when it is the one shown. */
    removeFromAlbum: async (id: number, paths: string[]) => {
      const changed = await albumChange(() => ipc.removeFromAlbum(id, paths));
      if (changed) {
        const gone = new Set(paths);
        setAlbum((shown) => (shown?.album.id === id ? { album: changed, photos: shown.photos.filter((p) => !gone.has(p.path)) } : shown));
      }
      return changed;
    },
    /** The photos an action applies to: the selected one (if in view) and the ticked
     *  ones, as one selection (click one, ⌘-click others, as in the Finder). */
    targets: (): string[] => {
      const inView = selected !== null && photos.some((p) => p.path === selected);
      return inView && !batchInView.includes(selected) ? [selected, ...batchInView] : batchInView;
    },
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
