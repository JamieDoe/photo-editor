import { useState, type DragEvent, type FormEvent } from "react";
import { AlbumIcon, PlusIcon, TrashIcon } from "../../components/icons";
import { Popover, PopoverHeader, PopoverIcon } from "../../components/Popover";
import type { AlbumDto } from "../../ipc/generated/AlbumDto";
import { NavGroup } from "./NavGroup";
import { PhotoThumbnail } from "./PhotoThumbnail";
import type { LibraryApi } from "./useLibrary";

/** Albums (ADR 0055): the photographer's own groups of photos, in the sidebar as the
 *  design has them, filled by dragging photos onto them or from "Add to album". */

/** Dragged photos carry their paths under this type. */
const PHOTO_PATHS = "application/x-photo-paths";

/** Starts dragging `paths`, `dragged` being the photo the pointer took. Several photos
 *  show as a stack of cards (the dragged one on top) with their count. */
export function setDraggedPaths(e: DragEvent, paths: string[], dragged: string) {
  e.dataTransfer.setData(PHOTO_PATHS, JSON.stringify(paths));
  e.dataTransfer.effectAllowed = "copy";
  if (paths.length < 2) return;
  const stack = dragStack(paths, dragged);
  // WebKit draws the drag image from the element as shown, so it is placed on screen
  // under the pointer for this moment, then removed.
  stack.style.left = `${e.clientX - STACK_GRAB[0]}px`;
  stack.style.top = `${e.clientY - STACK_GRAB[1]}px`;
  document.body.appendChild(stack);
  e.dataTransfer.setDragImage(stack, STACK_GRAB[0], STACK_GRAB[1]);
  window.setTimeout(() => stack.remove(), 0);
}

/** Where the pointer holds the stack (px from its top left). */
const STACK_GRAB = [52, 40] as const;

/** Up to three cards, the dragged photo on top and those whose thumbnails are on screen
 *  under it, and a badge with how many photos. */
function dragStack(paths: string[], dragged: string): HTMLElement {
  const picture = (path: string) =>
    document.querySelector<HTMLImageElement>(`[data-photo-path="${CSS.escape(path)}"] img`)?.src ?? null;
  const others = paths.filter((p) => p !== dragged).map(picture);
  const pictures = [picture(dragged), ...others.filter((s) => s !== null), ...others.filter((s) => s === null)].slice(0, 3);
  const stack = document.createElement("div");
  stack.className = "drag-stack";
  // Back to front: the last drawn is on top.
  pictures
    .map((src, i) => ({ src, depth: i }))
    .reverse()
    .forEach(({ src, depth }) => {
      const card = document.createElement("div");
      card.className = `drag-card depth-${depth}`;
      if (src) {
        const img = document.createElement("img");
        img.src = src;
        card.appendChild(img);
      }
      stack.appendChild(card);
    });
  const count = document.createElement("span");
  count.className = "drag-count";
  count.textContent = String(paths.length);
  stack.appendChild(count);
  return stack;
}

function draggedPaths(e: DragEvent): string[] {
  try {
    const v: unknown = JSON.parse(e.dataTransfer.getData(PHOTO_PATHS) || "[]");
    return Array.isArray(v) ? v.filter((p): p is string => typeof p === "string") : [];
  } catch {
    return [];
  }
}

export const photosText = (n: number) => `${n.toLocaleString()} photo${n === 1 ? "" : "s"}`;

/** The sidebar's Albums: each with its cover, name and count; + makes a new one.
 *  Photos dropped on an album are added to it. */
export function AlbumsNav({ library, notify }: { library: LibraryApi; notify: (message: string) => void }) {
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");
  const [over, setOver] = useState<number | null>(null);
  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    if (await library.createAlbum(name, [])) {
      setNaming(false);
      setName("");
    }
  };
  const drop = async (e: DragEvent, album: AlbumDto) => {
    e.preventDefault();
    setOver(null);
    const paths = draggedPaths(e);
    if (paths.length === 0) return;
    const before = album.count;
    const after = await library.addToAlbum(album.id, paths);
    if (after) notify(after.count === before ? `Already in ${after.name}` : `Added ${photosText(after.count - before)} to ${after.name}`);
  };
  const startNaming = () => setNaming(true);
  return (
    <NavGroup
      label="Albums"
      action={
        <button className="icon-button" aria-label="New album" title="New album" onClick={startNaming}>
          <PlusIcon size={13} />
        </button>
      }
    >
      {naming && (
        <form className="nav-new" onSubmit={(e) => void create(e)}>
          <input
            className="text-input"
            autoFocus
            maxLength={60}
            placeholder="Album name"
            aria-label="Album name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                setNaming(false);
                setName("");
              }
            }}
            onBlur={() => !name.trim() && setNaming(false)}
          />
        </form>
      )}
      {library.albums.length === 0 && !naming && (
        <div className="nav-empty-card">
          <AlbumIcon size={18} />
          <p>Group photos from any folder: a portfolio, prints, a trip.</p>
          <button className="ghost small" onClick={startNaming}>
            <PlusIcon size={12} />
            New album
          </button>
        </div>
      )}
      {library.albums.map((a) => (
        <button
          key={a.id}
          className={over === a.id ? "nav-row album-row drop" : "nav-row album-row"}
          aria-current={library.album?.album.id === a.id ? "true" : undefined}
          title={a.name}
          onClick={() => void library.openAlbum(a.id)}
          onDragOver={(e) => {
            if (!e.dataTransfer.types.includes(PHOTO_PATHS)) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
            setOver(a.id);
          }}
          onDragLeave={() => setOver((o) => (o === a.id ? null : o))}
          onDrop={(e) => void drop(e, a)}
        >
          <span className="album-cover">{a.cover && <PhotoThumbnail path={a.cover} className="album-cover-image" />}</span>
          <span className="grow">{a.name}</span>
          <span className="count">{a.count.toLocaleString()}</span>
        </button>
      ))}
      {library.albums.length > 0 && !naming && (
        <button className="nav-row nav-add" onClick={startNaming}>
          <PlusIcon size={14} />
          <span className="grow">New album</span>
        </button>
      )}
    </NavGroup>
  );
}

/** "Add to album": the albums to add `paths` to, or a new one made with them. */
export function AddToAlbum({
  library,
  paths,
  anchor,
  onClose,
  notify,
}: {
  library: LibraryApi;
  paths: string[];
  anchor: HTMLElement;
  onClose: () => void;
  notify: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const add = async (album: AlbumDto) => {
    const after = await library.addToAlbum(album.id, paths);
    if (after) notify(after.count === album.count ? `Already in ${after.name}` : `Added ${photosText(after.count - album.count)} to ${after.name}`);
    onClose();
  };
  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    const made = await library.createAlbum(name, paths);
    if (made) notify(`Added ${photosText(made.count)} to ${made.name}`);
    onClose();
  };
  return (
    <Popover anchor={anchor} label="Add to album" onClose={onClose}>
      <PopoverHeader
        visual={
          <PopoverIcon>
            <AlbumIcon size={18} />
          </PopoverIcon>
        }
        title={<span className="popover-title">Add to album</span>}
        sub={photosText(paths.length)}
      />
      {library.albums.length > 0 && (
        <div className="popover-menu album-menu">
          {library.albums.map((a) => (
            <button key={a.id} className="popover-item" onClick={() => void add(a)}>
              <span className="album-cover">{a.cover && <PhotoThumbnail path={a.cover} className="album-cover-image" />}</span>
              <span className="grow">{a.name}</span>
              <span className="count">{a.count.toLocaleString()}</span>
            </button>
          ))}
        </div>
      )}
      <form className="popover-body" onSubmit={(e) => void create(e)}>
        <input
          className="text-input"
          autoFocus={library.albums.length === 0}
          maxLength={60}
          placeholder="New album name"
          aria-label="New album name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <div className="popover-actions">
          <button className="primary small" type="submit" disabled={!name.trim()}>
            New album
          </button>
        </div>
      </form>
    </Popover>
  );
}

/** The shown album's own actions: rename it, or delete it (after asking). */
export function AlbumSettings({
  library,
  album,
  anchor,
  onClose,
}: {
  library: LibraryApi;
  album: AlbumDto;
  anchor: HTMLElement;
  onClose: () => void;
}) {
  const [name, setName] = useState(album.name);
  const [confirming, setConfirming] = useState(false);
  const rename = async (e: FormEvent) => {
    e.preventDefault();
    if (name.trim() && name.trim() !== album.name) await library.renameAlbum(album.id, name);
    onClose();
  };
  return (
    <Popover anchor={anchor} label={`Album ${album.name}`} onClose={onClose}>
      <PopoverHeader
        visual={album.cover ? <PhotoThumbnail path={album.cover} className="popover-thumb" /> : <PopoverIcon><AlbumIcon size={18} /></PopoverIcon>}
        title={<span className="popover-title">{album.name}</span>}
        sub={`${photosText(album.count)} · deleting an album keeps its photos`}
      />
      <form className="popover-body" onSubmit={(e) => void rename(e)}>
        <input className="text-input" maxLength={60} aria-label="Album name" value={name} onChange={(e) => setName(e.target.value)} />
        <div className="popover-actions">
          {confirming ? (
            <>
              <span className="popover-text small grow">Delete this album?</span>
              <button className="ghost small" type="button" onClick={() => setConfirming(false)}>
                Keep
              </button>
              <button
                className="danger small"
                type="button"
                onClick={() => {
                  void library.deleteAlbum(album.id);
                  onClose();
                }}
              >
                Delete
              </button>
            </>
          ) : (
            <>
              <button className="ghost small album-delete" type="button" onClick={() => setConfirming(true)}>
                <TrashIcon size={13} />
                Delete
              </button>
              <button className="primary small" type="submit" disabled={!name.trim()}>
                Save
              </button>
            </>
          )}
        </div>
      </form>
    </Popover>
  );
}
