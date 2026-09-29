import { useRef, type RefObject } from "react";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { formatBytes, formatCaptured, formatDateTime } from "../../lib/format";
import { PhotoThumbnail } from "./PhotoThumbnail";
import { useVirtualRows } from "./useVirtualRows";

interface Props {
  photos: PhotoEntryDto[];
  scrollRef: RefObject<HTMLElement | null>;
  onOpen: (path: string) => void;
}

const ROW_HEIGHT = 66;

/** Details list: one row per photo with capture time and camera. Virtualised. */
export function PhotoList({ photos, scrollRef, onOpen }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const { first, end } = useVirtualRows(scrollRef, listRef, ROW_HEIGHT, photos.length, 4);
  return (
    <div className="photo-list" role="table" aria-label="Photos" aria-rowcount={photos.length + 1}>
      <div className="list-row list-head" role="row">
        <span role="columnheader" aria-label="Thumbnail" />
        <span role="columnheader">Name</span>
        <span role="columnheader">Taken</span>
        <span role="columnheader">Camera</span>
        <span role="columnheader">Type</span>
        <span role="columnheader" className="num">
          Size
        </span>
      </div>
      <div ref={listRef} style={{ height: photos.length * ROW_HEIGHT, position: "relative" }}>
        <div style={{ transform: `translateY(${first * ROW_HEIGHT}px)` }}>
          {photos.slice(first, end).map((p, i) => (
            <div key={p.path} className="list-row" role="row" aria-rowindex={first + i + 2} style={{ height: ROW_HEIGHT }}>
              <span role="cell">
                <button className="thumb-button" onClick={() => onOpen(p.path)} aria-label={`Open ${p.name} in Edit`}>
                  <PhotoThumbnail path={p.path} />
                </button>
              </span>
              <span role="cell">
                <button className="text-button name" onClick={() => onOpen(p.path)} title="Open in Edit">
                  {p.name}
                </button>
              </span>
              <span role="cell" title={p.details?.capturedAt ? "Capture time recorded by the camera" : "File date (not indexed yet)"}>
                {formatCaptured(p.details?.capturedAt) ?? <span className="file-date">{formatDateTime(p.modifiedMs)}</span>}
              </span>
              <span role="cell" title={p.details?.lens ?? undefined}>
                {p.details?.camera ?? <span className="muted">—</span>}
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
