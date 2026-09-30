import { useEffect, useRef, useState } from "react";
import { requestThumbnail } from "./thumbnailLoader";

/**
 * A photo's thumbnail. The Library views are virtualised, so this is mounted only for
 * rows on or near the screen: it requests the thumbnail on mount, and unmounting
 * (scrolled away) cancels a pending request or frees the image. A new `rev` (the
 * photo's edit changed) fetches it again.
 */
export function PhotoThumbnail({ path, rev = 0, className = "thumb" }: { path: string; rev?: number; className?: string }) {
  const [url, setUrl] = useState<string | null>(null);
  const shownPath = useRef(path);

  useEffect(() => {
    // Another photo starts blank; a new revision of the same one (its edit changed)
    // keeps showing the old picture until the new one arrives.
    if (shownPath.current !== path) setUrl(null);
    shownPath.current = path;
    const release = requestThumbnail(path, setUrl);
    return release;
  }, [path, rev]);
  useEffect(() => () => setUrl(null), []);

  return <span className={className}>{url && <img src={url} alt="" decoding="async" draggable={false} />}</span>;
}
