import { useEffect, useRef, useState } from "react";
import { observeVisibility } from "../../lib/visibility";
import { requestThumbnail } from "./thumbnailLoader";

/**
 * A photo's thumbnail, requested only while it is on screen. Scrolling it away
 * cancels a pending request and frees the image; coming back is a disk-cache hit.
 */
export function PhotoThumbnail({ path }: { path: string }) {
  const box = useRef<HTMLSpanElement>(null);
  const [visible, setVisible] = useState(false);
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    const el = box.current;
    return el ? observeVisibility(el, setVisible) : undefined;
  }, []);

  useEffect(() => {
    if (!visible) return;
    const release = requestThumbnail(path, setUrl);
    return () => {
      release();
      setUrl(null);
    };
  }, [visible, path]);

  return (
    <span ref={box} className="thumb">
      {url && <img src={url} alt="" decoding="async" draggable={false} />}
    </span>
  );
}
