import { useRef, type RefObject } from "react";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { formatCaptured } from "../../lib/format";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { useVirtualRows, useWidth } from "./useVirtualRows";
import { GRID_GAP_X, gridLayout } from "./virtual";

interface Props {
  photos: PhotoEntryDto[];
  scrollRef: RefObject<HTMLElement | null>;
  onOpen: (path: string) => void;
}

/** Thumbnail grid (3:2 cards, as in the design). Only rows near the view are rendered. */
export function PhotoGrid({ photos, scrollRef, onOpen }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const width = useWidth(listRef);
  const { columns, rowHeight } = gridLayout(width);
  const rowCount = Math.ceil(photos.length / columns);
  const { first, end } = useVirtualRows(scrollRef, listRef, rowHeight, rowCount, 1);

  const rows = [];
  for (let r = first; r < end; r++) {
    rows.push(
      <div key={r} className="grid-row" style={{ height: rowHeight, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`, columnGap: GRID_GAP_X }}>
        {photos.slice(r * columns, (r + 1) * columns).map((p) => (
          <button
            key={p.path}
            className="card"
            onClick={() => onOpen(p.path)}
            title={[p.name, p.details?.camera, formatCaptured(p.details?.capturedAt)].filter(Boolean).join("\n")}
          >
            <PhotoThumbnail path={p.path} className="card-image" />
            <span className="card-caption">
              <span className="card-name">{p.name}</span>
              <span className="card-type">{p.raw ? "RAW" : "JPEG"}</span>
            </span>
          </button>
        ))}
      </div>,
    );
  }
  return (
    <div ref={listRef} className="photo-grid" role="list" aria-label="Photos" style={{ height: rowCount * rowHeight }}>
      <div style={{ transform: `translateY(${first * rowHeight}px)` }}>{rows}</div>
    </div>
  );
}
