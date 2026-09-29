import { useCallback, useEffect, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { FolderListingDto } from "../../ipc/generated/FolderListingDto";
import type { IndexEvent } from "../../ipc/generated/IndexEvent";
import type { LibraryStatusDto } from "../../ipc/generated/LibraryStatusDto";

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
  // Kept here (not in the view) so it survives switching to Edit and back.
  const [layout, setLayout] = useState<LibraryLayout>("grid");
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

  const openFolder = useCallback((path: string) => load(() => ipc.listFolder(path)), [load]);

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
    chooseFolder,
    openFolder,
    refresh,
  };
}

export type LibraryApi = ReturnType<typeof useLibrary>;
