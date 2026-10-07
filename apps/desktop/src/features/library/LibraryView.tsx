import { useCallback, useEffect, useRef, useState, type DragEvent, type MouseEvent } from "react";
import { AlbumIcon, CloseIcon, FolderIcon, SearchIcon, ImportIcon, MoreIcon, PhotosIcon, PickIcon, RatingStar, RefreshIcon, RejectIcon, StarIcon } from "../../components/icons";
import type { CollectionKindDto } from "../../ipc/generated/CollectionKindDto";
import * as ipc from "../../ipc/client";
import { formatDateRange } from "../../lib/format";
import { hasCommandModifier, isTextEntry } from "../../lib/keyboard";
import type { SettingsApi } from "../settings/useSettings";
import { AddToAlbum, AlbumSettings, AlbumsNav, photosText, setDraggedPaths } from "./Albums";
import { indexStatusText } from "./indexStatus";
import { NavGroup } from "./NavGroup";
import { LabelFilter } from "../../components/ColourLabels";
import { COLLECTION_NAMES, FILTERS, LABELS, SORTS, markChangeForKey, type Label, type LibraryFilter, type LibrarySort } from "./marks";
import { PhotoGrid } from "./PhotoGrid";
import { PhotoList } from "./PhotoList";
import type { LibraryApi, LibraryLayout } from "./useLibrary";

interface Props {
  library: LibraryApi;
  settings: SettingsApi;
  onOpenPhoto: (path: string) => void;
  notify: (message: string) => void;
}

let defaultFolderTried = false;

const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/** Library mode: browse granted folders and open a photo. */
export function LibraryView({ library, settings, onOpenPhoto, notify }: Props) {
  const { listing, loading } = library;
  const s = settings.settings;
  const defaultFolder = s?.library.defaultFolder ?? null;

  // Show the default folder the first time the Library appears.
  useEffect(() => {
    if (defaultFolderTried || listing || !defaultFolder) return;
    defaultFolderTried = true;
    void library.openFolder(defaultFolder);
  }, [defaultFolder, listing, library]);

  const indexText = indexStatusText(library.indexing, library.lastIndex);

  const choose = async () => {
    const result = await library.chooseFolder();
    if (result) await settings.reload(); // recent folders changed in Rust
  };

  const makeDefault = async () => {
    if (!listing) return;
    try {
      settings.adopt(await ipc.setDefaultFolder(listing.path));
    } catch {
      /* the listing error path already covers unavailable folders */
    }
  };

  const scrollRef = useRef<HTMLDivElement>(null);
  const columnsRef = useRef(1);
  const onColumns = useCallback((n: number) => {
    columnsRef.current = n;
  }, []);
  const { visible, selected, setSelected, setMarks, album, search } = library;
  // A search shows over the view underneath, which comes back when it is cleared.
  const collection = search ? null : library.collection;
  const albumShown = search ? null : album;
  const [popover, setPopover] = useState<{ kind: "add" | "album"; anchor: HTMLElement } | null>(null);
  const ticked = new Set(library.batch);
  const targets = library.targets();
  /** ⌘-click ticks a photo, ⇧-click the range to it (the filmstrip's ticks, ADR 0049);
   *  a plain click selects it. */
  const pick = (path: string, e: MouseEvent) => {
    if (e.metaKey || e.ctrlKey) library.toggleBatch(path);
    else if (e.shiftKey) library.tickRange(path);
    else setSelected(path);
  };
  /** Dragging one of the selected and ticked photos drags them all; another, just it. */
  const drag = (path: string, e: DragEvent) => setDraggedPaths(e, targets.includes(path) ? targets : [path], path);

  // Keyboard: arrows move the selection, Enter opens, 0–5 / P / X / U mark it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (hasCommandModifier(e) || isTextEntry(e.target) || visible.length === 0) return;
      const i = visible.findIndex((p) => p.path === selected);
      const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: 0, ArrowUp: 0 }[e.key];
      if (step !== undefined) {
        const columns = library.layout === "grid" ? columnsRef.current : 1;
        const delta = e.key === "ArrowDown" ? columns : e.key === "ArrowUp" ? -columns : step;
        const next = i < 0 ? 0 : Math.min(visible.length - 1, Math.max(0, i + delta));
        setSelected(visible[next]!.path);
        e.preventDefault();
        return;
      }
      if (i < 0) return;
      if (e.key === "Enter") {
        onOpenPhoto(visible[i]!.path);
        e.preventDefault();
        return;
      }
      const change = markChangeForKey(e.key, visible[i]!.marks);
      if (change) {
        void setMarks([visible[i]!.path], change);
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, selected, setSelected, setMarks, onOpenPhoto, library.layout]);

  const counts = library.status?.collections;
  const collectionRow = (kind: CollectionKindDto, icon: React.ReactNode, count: number | undefined) => (
    <button
      key={kind}
      className="nav-row"
      aria-current={collection?.kind === kind ? "true" : undefined}
      onClick={() => void library.openCollection(kind)}
    >
      {icon}
      <span className="grow">{COLLECTION_NAMES[kind]}</span>
      <span className="count">{(count ?? 0).toLocaleString()}</span>
    </button>
  );
  const recent = s?.library.recentFolders ?? [];
  const currentRoot = listing?.breadcrumbs[0]?.path;
  const shownCount = visible.length;
  const photoCount = `${shownCount.toLocaleString()} photo${shownCount === 1 ? "" : "s"}${
    shownCount !== library.photos.length ? ` of ${library.photos.length.toLocaleString()}` : ""
  }`;
  const dateRange = formatDateRange(visible.map((p) => p.details?.capturedAt));

  return (
    <div className="library-view">
      <aside className="sidebar-left">
        <div className="sidebar-search">
          <label className="search-field">
            <SearchIcon size={15} />
            <input
              type="search"
              placeholder="Search folders, cameras, dates"
              aria-label="Search photos"
              value={library.query}
              onChange={(e) => library.runSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  library.clearSearch();
                  e.currentTarget.blur();
                }
              }}
            />
            {library.query && (
              <button className="search-clear" aria-label="Clear search" title="Clear search" onClick={library.clearSearch}>
                <CloseIcon size={11} />
              </button>
            )}
          </label>
        </div>
        <div className="sidebar-scroll scroll">
          <section className="nav-section" aria-label="Library">
            <div className="nav-label">Library</div>
            {collectionRow("all", <PhotosIcon />, counts?.all)}
            {collectionRow("recent", <ImportIcon size={16} />, counts?.recent)}
            {collectionRow("picks", <PickIcon size={16} />, counts?.picks)}
            {collectionRow("rated", <RatingStar filled={false} />, counts?.rated)}
            {collectionRow("rejected", <RejectIcon size={16} />, counts?.rejected)}
          </section>
          <NavGroup
            label="Folders"
            action={
              <button className="icon-button" aria-label="Add folder" title="Add folder" onClick={() => void choose()}>
                <svg className="icon" width="13" height="13" viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M8 3v10M3 8h10" />
                </svg>
              </button>
            }
          >
            {recent.length === 0 && <p className="nav-empty">No folders yet.</p>}
            {recent.map((f) => (
              <button
                key={f}
                className="nav-row"
                aria-current={!collection && !albumShown && !search && currentRoot === f ? "true" : undefined}
                title={f}
                onClick={() => void library.openFolder(f)}
              >
                <FolderIcon />
                <span className="grow">{folderName(f)}</span>
                {f === defaultFolder && (
                  <span className="default-mark" title="Opens at start-up">
                    <StarIcon />
                  </span>
                )}
              </button>
            ))}
          </NavGroup>
          <AlbumsNav library={library} notify={notify} />
        </div>
        <div className="sidebar-footer">
          <button className="block" onClick={() => void choose()}>
            <ImportIcon />
            Choose folder…
          </button>
          <div className="status-line" aria-live="polite">
            <span className={library.indexing ? "status-dot busy" : "status-dot"} />
            {indexText ?? "Originals are never modified"}
          </div>
        </div>
      </aside>

      <section className="library-main">
        {library.status?.notice && <p className="notice library-notice">{library.status.notice}</p>}
        {!listing && !collection && !album && !search ? (
          <div className="empty-state">
            {loading ? (
              <p>Loading…</p>
            ) : (
              <>
                <h2>Add a folder of photos</h2>
                <p>Your photos stay where they are. Nothing is copied or changed.</p>
                <button className="primary" onClick={() => void choose()}>
                  <ImportIcon />
                  Choose folder…
                </button>
              </>
            )}
          </div>
        ) : (
          <>
            <header className="page-header">
              <div className="page-title">
                {!collection && !albumShown && !search && listing && listing.breadcrumbs.length > 1 && (
                  <nav className="crumbs" aria-label="Folder">
                    {listing.breadcrumbs.slice(0, -1).map((c) => (
                      <span key={c.path}>
                        <button className="text-button" onClick={() => void library.openFolder(c.path)}>
                          {c.name}
                        </button>
                        <span className="sep">/</span>
                      </span>
                    ))}
                  </nav>
                )}
                {(collection || search) && <span className="crumbs">{search ? "Search · across all folders" : "Across all folders"}</span>}
                {albumShown && <span className="crumbs">Album</span>}
                <div className="page-title-row">
                  <h1>
                    {search
                      ? `“${search.query.trim()}”`
                      : albumShown
                        ? albumShown.album.name
                        : collection
                          ? COLLECTION_NAMES[collection.kind]
                          : listing?.name}
                  </h1>
                  <span className="subtle">
                    {[photoCount, dateRange].filter(Boolean).join(" · ")}
                    {loading ? " · loading…" : ""}
                  </span>
                </div>
              </div>
              <div className="page-actions">
                {indexText && <span className="index-status">{indexText}</span>}
                {library.batch.length > 0 && (
                  <span className="ticked-count">
                    {photosText(targets.length)} selected
                    <button className="icon-button" aria-label="Clear selection" title="Clear selection" onClick={library.clearBatch}>
                      <CloseIcon size={12} />
                    </button>
                  </span>
                )}
                {albumShown && targets.length > 0 && (
                  <button
                    className="ghost"
                    onClick={async () => {
                      const n = targets.length;
                      if (await library.removeFromAlbum(albumShown.album.id, targets)) {
                        library.clearBatch();
                        notify(`Removed ${photosText(n)} from ${albumShown.album.name}`);
                      }
                    }}
                  >
                    Remove from album
                  </button>
                )}
                {targets.length > 0 && (
                  <button
                    className="ghost"
                    aria-expanded={popover?.kind === "add"}
                    title="Add the selected or ticked photos to an album"
                    onClick={(e) => setPopover({ kind: "add", anchor: e.currentTarget })}
                  >
                    <AlbumIcon />
                    Add to album
                  </button>
                )}
                {albumShown && (
                  <button
                    className="icon-button"
                    aria-label="Rename or delete this album"
                    title="Rename or delete this album"
                    aria-expanded={popover?.kind === "album"}
                    onClick={(e) => setPopover({ kind: "album", anchor: e.currentTarget })}
                  >
                    <MoreIcon />
                  </button>
                )}
                {!collection && !albumShown && !search && listing && (
                  <>
                    <button className="ghost" onClick={() => void library.refresh()}>
                      <RefreshIcon />
                      Refresh
                    </button>
                    {listing.path !== defaultFolder && (
                      <button className="ghost" onClick={() => void makeDefault()}>
                        Set as default
                      </button>
                    )}
                  </>
                )}
                <Segmented
                  name="library-filter"
                  label="Filter"
                  options={FILTERS}
                  value={library.filter}
                  onChange={library.setFilter}
                />
                <LabelFilter value={library.labelFilter} onChange={library.setLabelFilter} />
                <select
                  className="sort-select"
                  aria-label="Sort by"
                  title="Sort by"
                  value={library.sort}
                  onChange={(e) => library.setSort(e.target.value as LibrarySort)}
                >
                  {SORTS.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.label}
                    </option>
                  ))}
                </select>
                <Segmented
                  name="library-layout"
                  label="Layout"
                  options={LAYOUTS}
                  value={library.layout}
                  onChange={library.setLayout}
                />
              </div>
            </header>

            {/* Keyed by view: a newly opened folder or collection starts at the top. */}
            <div
              className="library-scroll scroll"
              ref={scrollRef}
              key={search ? "search" : albumShown ? `album:${albumShown.album.id}` : collection ? `collection:${collection.kind}` : listing?.path}
            >
              {!collection && !albumShown && !search && listing && listing.folders.length > 0 && (
                <div className="chips">
                  {listing.folders.map((f) => (
                    <button key={f.path} className="chip" onClick={() => void library.openFolder(f.path)}>
                      <FolderIcon size={14} />
                      {f.name}
                    </button>
                  ))}
                </div>
              )}
              {visible.length === 0 ? (
                <p className="muted empty-note">
                  {search
                    ? library.photos.length === 0
                      ? `Nothing matches “${search.query.trim()}”. Try a folder or place, a camera, a lens, a year or a month.`
                      : "No photos match this filter."
                    : albumShown
                    ? library.photos.length === 0
                      ? "Nothing here yet. Drag photos onto the album in the sidebar, or select some and use Add to album."
                      : "No photos match this filter."
                    : emptyMessage(library.photos.length, library.filter, collection?.kind ?? null, library.labelFilter)}
                </p>
              ) : library.layout === "grid" ? (
                <PhotoGrid
                  photos={visible}
                  scrollRef={scrollRef}
                  selected={selected}
                  onPick={pick}
                  onOpen={onOpenPhoto}
                  ticked={ticked}
                  onDrag={drag}
                  onColumns={onColumns}
                />
              ) : (
                <PhotoList
                  photos={visible}
                  scrollRef={scrollRef}
                  selected={selected}
                  onPick={pick}
                  onOpen={onOpenPhoto}
                  ticked={ticked}
                  onDrag={drag}
                />
              )}
              {!collection && !albumShown && !search && listing && listing.skipped > 0 && (
                <p className="muted">{listing.skipped} item(s) couldn’t be read and are not shown.</p>
              )}
            </div>
          </>
        )}
      </section>
      {popover?.kind === "add" && (
        <AddToAlbum library={library} paths={targets} anchor={popover.anchor} onClose={() => setPopover(null)} notify={notify} />
      )}
      {popover?.kind === "album" && albumShown && (
        <AlbumSettings library={library} album={albumShown.album} anchor={popover.anchor} onClose={() => setPopover(null)} />
      )}
    </div>
  );
}

/** Recently imported reaches back this far (the catalogue's `RECENT_DAYS`). */
const RECENT_DAYS = 30;

const LAYOUTS: ReadonlyArray<{ id: LibraryLayout; label: string }> = [
  { id: "grid", label: "Grid" },
  { id: "list", label: "List" },
];

function emptyMessage(total: number, filter: LibraryFilter, collection: CollectionKindDto | null, label: Label | null): string {
  if (label !== null && total > 0) {
    const l = LABELS.find((x) => x.id === label)!;
    return `No photos labelled ${l.name.toLowerCase()} here.${l.key ? ` Select a photo and press ${l.key}.` : ""}`;
  }
  if (collection) {
    const how: Record<CollectionKindDto, string> = {
      all: "Add a folder, and its photos show here once indexed.",
      picks: "Press P to pick the selected photo.",
      rated: "Press 1–5 to rate the selected photo.",
      rejected: "Press X to reject the selected photo.",
      recent: `Photos added to the library in the last ${RECENT_DAYS} days show here.`,
    };
    return total === 0 ? `Nothing here yet. ${how[collection]}` : "No photos match this filter.";
  }
  if (total === 0) return "No supported photos in this folder.";
  if (filter === "picks") return "No picks in this folder yet. Select a photo and press P.";
  if (filter === "rated3") return "No photos rated 3 stars or more here yet. Select a photo and press 3, 4 or 5.";
  return "No photos match this filter.";
}

/** A small segmented radio group (design's filter control). */
function Segmented<T extends string>({
  name,
  label,
  options,
  value,
  onChange,
}: {
  name: string;
  label: string;
  options: ReadonlyArray<{ id: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented small" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <label key={o.id}>
          <input className="sr-only" type="radio" name={name} checked={value === o.id} onChange={() => onChange(o.id)} />
          {o.label}
        </label>
      ))}
    </div>
  );
}
