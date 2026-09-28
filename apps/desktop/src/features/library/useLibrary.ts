import { useCallback, useRef, useState } from "react";
import { toAppError, type AppError } from "../../app/errors";
import * as ipc from "../../ipc/client";
import type { FolderListingDto } from "../../ipc/generated/FolderListingDto";

/**
 * Library browsing state. Folder access is enforced in Rust: only folders chosen in
 * the native dialog (and remembered ones) can be listed.
 */
export function useLibrary() {
  const [listing, setListing] = useState<FolderListingDto | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const requestRef = useRef(0);

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

  const chooseFolder = useCallback(() => load(ipc.chooseFolder), [load]);
  const openFolder = useCallback((path: string) => load(() => ipc.listFolder(path)), [load]);
  const refresh = useCallback(() => (listing ? openFolder(listing.path) : Promise.resolve(null)), [listing, openFolder]);

  return { listing, loading, error, clearError: () => setError(null), chooseFolder, openFolder, refresh };
}

export type LibraryApi = ReturnType<typeof useLibrary>;
