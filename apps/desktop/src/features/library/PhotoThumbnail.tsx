import { useEffect, useState } from "react";
import { requestThumbnail } from "./thumbnailLoader";

/**
 * A photo's thumbnail. The Library views are virtualised, so this is mounted only for
 * rows on or near the screen: it requests the thumbnail on mount, and unmounting
 * (scrolled away) cancels a pending request or frees the image.
 */
export function PhotoThumbnail({ path, className = "thumb" }: { path: string; className?: string }) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    const release = requestThumbnail(path, setUrl);
    return () => {
      release();
      setUrl(null);
    };
  }, [path]);

  return <span className={className}>{url && <img src={url} alt="" decoding="async" draggable={false} />}</span>;
}
