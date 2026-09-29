import { useEffect, useRef, type RefObject } from "react";
import { PickIcon } from "../../components/icons";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { formatCaptured } from "../../lib/format";
import { starsText } from "./marks";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { useRevealRow } from "./useRevealRow";
import { useVirtualRows, useWidth } from "./useVirtualRows";
import { GRID_GAP_X, gridLayout } from "./virtual";

interface Props {
  photos: PhotoEntryDto[];
  scrollRef: RefObject<HTMLElement | null>;
  selected: string | null;
  onSelect: (path: string) => void;
  onOpen: (path: string) => void;
  /** Reports the column count, for up/down keyboard movement. */
  onColumns: (columns: number) => void;
}

/**
 * Thumbnail grid (3:2 cards, as in the design). Only rows near the view are rendered.
 * Click selects, double-click opens.
 */
export function PhotoGrid({ photos, scrollRef, selected, onSelect, onOpen, onColumns }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const width = useWidth(listRef);
  const { columns, rowHeight } = gridLayout(width);
  const rowCount = Math.ceil(photos.length / columns);
  const { first, end } = useVirtualRows(scrollRef, listRef, rowHeight, rowCount, 1);
  const selectedIndex = selected ? photos.findIndex((p) => p.path === selected) : -1;
  useRevealRow(scrollRef, listRef, selectedIndex >= 0 ? Math.floor(selectedIndex / columns) : null, rowHeight);
  useEffect(() => {
    onColumns(columns);
  }, [columns, onColumns]);

  const rows = [];
  for (let r = first; r < end; r++) {
    rows.push(
      <div key={r} className="grid-row" style={{ height: rowHeight, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`, columnGap: GRID_GAP_X }}>
        {photos.slice(r * columns, (r + 1) * columns).map((p) => (
          <button
            key={p.path}
            className={p.marks.flag === "reject" ? "card rejected" : "card"}
            aria-pressed={p.path === selected}
            onClick={() => onSelect(p.path)}
            onDoubleClick={() => onOpen(p.path)}
            title={[p.name, p.details?.camera, formatCaptured(p.details?.capturedAt), "Double-click to edit"].filter(Boolean).join("\n")}
          >
            <span className="card-frame">
              <PhotoThumbnail path={p.path} className="card-image" />
              {p.marks.flag === "pick" && (
                <span className="card-badge" aria-label="Pick">
                  <PickIcon size={12} />
                </span>
              )}
            </span>
            <span className="card-caption">
              <span className="card-name">{p.name}</span>
              {p.marks.rating > 0 ? (
                <span className="card-stars" aria-label={`${p.marks.rating} stars`}>
                  {starsText(p.marks.rating)}
                </span>
              ) : (
                <span className="card-type">{p.raw ? "RAW" : "JPEG"}</span>
              )}
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
