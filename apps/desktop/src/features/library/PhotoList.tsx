import { useRef, type RefObject } from "react";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { PickIcon, RejectIcon } from "../../components/icons";
import { formatBytes, formatCaptured, formatDateTime } from "../../lib/format";
import { starsText } from "./marks";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { useRevealRow } from "./useRevealRow";
import { useVirtualRows } from "./useVirtualRows";

interface Props {
  photos: PhotoEntryDto[];
  scrollRef: RefObject<HTMLElement | null>;
  selected: string | null;
  onSelect: (path: string) => void;
  onOpen: (path: string) => void;
}

const ROW_HEIGHT = 66;

/** Details list: one row per photo with capture time, camera and marks. Virtualised.
 * Click selects, double-click opens. */
export function PhotoList({ photos, scrollRef, selected, onSelect, onOpen }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const { first, end } = useVirtualRows(scrollRef, listRef, ROW_HEIGHT, photos.length, 4);
  const selectedIndex = selected ? photos.findIndex((p) => p.path === selected) : -1;
  useRevealRow(scrollRef, listRef, selectedIndex >= 0 ? selectedIndex : null, ROW_HEIGHT);
  return (
    <div className="photo-list" role="table" aria-label="Photos" aria-rowcount={photos.length + 1}>
      <div className="list-row list-head" role="row">
        <span role="columnheader" aria-label="Thumbnail" />
        <span role="columnheader">Name</span>
        <span role="columnheader">Taken</span>
        <span role="columnheader">Camera</span>
        <span role="columnheader">Rating</span>
        <span role="columnheader">Type</span>
        <span role="columnheader" className="num">
          Size
        </span>
      </div>
      <div ref={listRef} style={{ height: photos.length * ROW_HEIGHT, position: "relative" }}>
        <div style={{ transform: `translateY(${first * ROW_HEIGHT}px)` }}>
          {photos.slice(first, end).map((p, i) => (
            <div
              key={p.path}
              className={["list-row", p.path === selected ? "selected" : "", p.marks.flag === "reject" ? "rejected" : ""].join(" ")}
              role="row"
              aria-rowindex={first + i + 2}
              aria-selected={p.path === selected}
              style={{ height: ROW_HEIGHT }}
              onClick={() => onSelect(p.path)}
              onDoubleClick={() => onOpen(p.path)}
            >
              <span role="cell">
                <button className="thumb-button" onDoubleClick={() => onOpen(p.path)} aria-label={`${p.name}; double-click to edit`}>
                  <PhotoThumbnail path={p.path} />
                </button>
              </span>
              <span role="cell">
                <button className="text-button name" onDoubleClick={() => onOpen(p.path)} title="Double-click to edit">
                  {p.name}
                </button>
              </span>
              <span role="cell" title={p.details?.capturedAt ? "Capture time recorded by the camera" : "File date (not indexed yet)"}>
                {formatCaptured(p.details?.capturedAt) ?? <span className="file-date">{formatDateTime(p.modifiedMs)}</span>}
              </span>
              <span role="cell" title={p.details?.lens ?? undefined}>
                {p.details?.camera ?? <span className="muted">—</span>}
              </span>
              <span role="cell" className="marks-cell">
                <span className="card-stars">{starsText(p.marks.rating)}</span>
                {p.marks.flag === "pick" && <PickIcon size={12} filled />}
                {p.marks.flag === "reject" && <RejectIcon size={12} />}
              </span>
              <span role="cell" className="type">
                {p.raw ? "RAW" : "JPEG"}
              </span>
              <span role="cell" className="num">
                {formatBytes(p.sizeBytes)}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
