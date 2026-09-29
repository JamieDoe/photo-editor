import * as ipc from "../../ipc/client";

export interface ThumbnailDeps {
  fetch: (path: string) => Promise<ArrayBuffer>;
  cancel: (path: string) => Promise<void>;
  createUrl: (blob: Blob) => string;
  revokeUrl: (url: string) => void;
}

const defaultDeps: ThumbnailDeps = {
  fetch: ipc.libraryThumbnail,
  cancel: ipc.cancelThumbnail,
  createUrl: (blob) => URL.createObjectURL(blob),
  revokeUrl: (url) => URL.revokeObjectURL(url),
};

/**
 * Requests the thumbnail of `path` and calls `onReady` with an object URL for it.
 * Returns a release function: before the thumbnail arrives it cancels the request in
 * Rust (the row scrolled away, so no work is spent on it); afterwards it frees the URL.
 * Failures leave the placeholder in place; they are logged, never shown per photo.
 */
export function requestThumbnail(
  path: string,
  onReady: (url: string) => void,
  deps: ThumbnailDeps = defaultDeps,
): () => void {
  let released = false;
  let url: string | null = null;
  deps.fetch(path).then(
    (bytes) => {
      if (released) return;
      url = deps.createUrl(new Blob([bytes], { type: "image/jpeg" }));
      onReady(url);
    },
    (e: unknown) => {
      if (!released && !ipc.isCancellation(e)) console.warn(`thumbnail unavailable for ${path}`, e);
    },
  );
  return () => {
    if (released) return;
    released = true;
    if (url) deps.revokeUrl(url);
    else void deps.cancel(path).catch(() => {});
  };
}
