import { useEffect, useRef, useState, type MouseEvent } from "react";
import { LabelDot } from "../../components/ColourLabels";
import { CheckIcon, CloseIcon, PickIcon, SyncIcon } from "../../components/icons";
import type { PhotoEntryDto } from "../../ipc/generated/PhotoEntryDto";
import { FILTERS } from "../library/marks";
import { PhotoThumbnail } from "../library/PhotoThumbnail";
import type { LibraryApi } from "../library/useLibrary";

/** A thumbnail's width and the gap after it (the design's), and the strip's side
 *  padding (px). The rings of the open and ticked photos are drawn inside a thumbnail's
 *  edge, so neighbouring rings keep the whole gap between them. */
const THUMB = 96;
const GAP = 8;
const STRIDE = THUMB + GAP;
const PADDING = 16;
/** Thumbnails kept mounted either side of those in view. */
const OVERSCAN = 4;

/**
 * The design's filmstrip under the photo (ADR 0049): the Library's photos as they are
 * filtered there, the open one ringed in white. Clicking one opens it; its box (or
 * ⌘-click, ⇧-click for a range) ticks it for batch editing, ringed in the accent colour.
 * With photos ticked, the header offers Sync edits and clearing. Only the thumbnails
 * near the view are mounted, so a folder of thousands asks for a few dozen.
 */
export function Filmstrip({
  library,
  current,
  onOpen,
  onSync,
  syncing,
  onExport,
}: {
  library: LibraryApi;
  /** The photo open in Edit, if it is one of the Library's. */
  current: string | null;
  onOpen: (path: string) => void;
  onSync: () => void;
  syncing: boolean;
  /** Opens the export dialog for the open and ticked photos (ADR 0050). */
  onExport: () => void;
}) {
  const photos = library.visible;
  const ticked = new Set(library.batch);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ left: 0, width: 0 });

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setView({ left: el.scrollLeft, width: el.clientWidth });
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    measure();
    return () => observer.disconnect();
  }, []);

  // Keep the open photo in view as it changes (← →, or opened from the Library).
  const currentIndex = photos.findIndex((p) => p.path === current);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || currentIndex < 0) return;
    const start = PADDING + currentIndex * STRIDE;
    if (start < el.scrollLeft || start + THUMB > el.scrollLeft + el.clientWidth) {
      el.scrollTo({ left: Math.max(0, start - (el.clientWidth - THUMB) / 2), behavior: "smooth" });
    }
  }, [currentIndex]);

  const first = Math.max(0, Math.floor((view.left - PADDING) / STRIDE) - OVERSCAN);
  const last = Math.min(photos.length, Math.ceil((view.left + view.width - PADDING) / STRIDE) + OVERSCAN);

  const click = (e: MouseEvent, path: string) => {
    if (e.shiftKey) library.tickRange(path);
    else if (e.metaKey || e.ctrlKey) library.toggleBatch(path);
    else onOpen(path);
  };

  return (
    <div className="filmstrip">
      <div className="filmstrip-header">
        <div className="segmented small filmstrip-filters" role="radiogroup" aria-label="Show">
          {FILTERS.map((f) => (
            <button key={f.id} role="radio" aria-checked={library.filter === f.id} onClick={() => library.setFilter(f.id)}>
              {f.label}
            </button>
          ))}
        </div>
        {library.batch.length > 0 ? (
          <div className="batch-bar">
            <span className="batch-count">
              <strong>{library.batch.length}</strong> selected
            </span>
            <button
              className="batch-sync"
              disabled={syncing}
              title="Give the ticked photos this photo’s edits (the groups Copy takes)"
              onClick={onSync}
            >
              <SyncIcon size={13} />
              {syncing ? "Syncing…" : "Sync edits"}
            </button>
            <button className="batch-export" title="Export the open and ticked photos" onClick={onExport}>
              Export…
            </button>
            <button className="batch-clear" aria-label="Clear selection" title="Clear selection" onClick={library.clearBatch}>
              <CloseIcon size={12} />
            </button>
          </div>
        ) : (
          <span className="filmstrip-hint">Tick photos to edit several at once</span>
        )}
      </div>
      <div
        ref={scrollRef}
        className="filmstrip-scroll"
        onScroll={(e) => setView({ left: e.currentTarget.scrollLeft, width: e.currentTarget.clientWidth })}
      >
        <div className="filmstrip-track" style={{ width: PADDING * 2 + photos.length * STRIDE - GAP }}>
          {photos.slice(first, last).map((p, i) => (
            <StripThumb
              key={p.path}
              photo={p}
              left={PADDING + (first + i) * STRIDE}
              rev={library.thumbRevs[p.path] ?? 0}
              current={p.path === current}
              ticked={ticked.has(p.path)}
              onClick={(e) => click(e, p.path)}
              onTick={() => library.toggleBatch(p.path)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function StripThumb({
  photo,
  left,
  rev,
  current,
  ticked,
  onClick,
  onTick,
}: {
  photo: PhotoEntryDto;
  left: number;
  rev: number;
  current: boolean;
  ticked: boolean;
  onClick: (e: MouseEvent) => void;
  onTick: () => void;
}) {
  const { rating, flag } = photo.marks;
  const classes = ["strip-thumb", current && "current", ticked && "ticked", flag === "reject" && "rejected"].filter(Boolean).join(" ");
  return (
    <div className={classes} style={{ left }}>
      <button className="strip-image" aria-label={photo.name} aria-current={current || undefined} title={photo.name} onClick={onClick}>
        <PhotoThumbnail path={photo.path} rev={rev} className="strip-picture" />
        <span className="strip-marks">
          <span className="strip-stars">{rating > 0 ? "★".repeat(rating) : ""}</span>
          <span className="strip-flags">
            <LabelDot label={photo.marks.label} />
            {photo.edited && <span className="strip-edited" title="Edited" />}
            {flag === "pick" && <PickIcon size={9} filled />}
          </span>
        </span>
      </button>
      <button
        className="strip-check"
        role="checkbox"
        aria-checked={ticked}
        aria-label={`Select ${photo.name} for batch editing`}
        title="Select for batch editing"
        onClick={onTick}
      >
        {ticked && <CheckIcon size={10} strokeWidth={2.6} />}
      </button>
    </div>
  );
}
