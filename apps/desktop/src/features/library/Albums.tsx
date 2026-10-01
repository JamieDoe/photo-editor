import { useState, type DragEvent, type FormEvent } from "react";
import { AlbumIcon, PlusIcon, TrashIcon } from "../../components/icons";
import { Popover, PopoverHeader, PopoverIcon } from "../../components/Popover";
import type { AlbumDto } from "../../ipc/generated/AlbumDto";
import { PhotoThumbnail } from "./PhotoThumbnail";
import type { LibraryApi } from "./useLibrary";

/** Albums (ADR 0055): the photographer's own groups of photos, in the sidebar as the
 *  design has them, filled by dragging photos onto them or from "Add to album". */

/** Dragged photos carry their paths under this type. */
const PHOTO_PATHS = "application/x-photo-paths";

export function setDraggedPaths(e: DragEvent, paths: string[]) {
  e.dataTransfer.setData(PHOTO_PATHS, JSON.stringify(paths));
  e.dataTransfer.effectAllowed = "copy";
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
  return (
    <section className="nav-section" aria-label="Albums">
      <div className="nav-label">
        Albums
        <button className="icon-button" aria-label="New album" title="New album" onClick={() => setNaming(true)}>
          <PlusIcon size={13} />
        </button>
      </div>
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
      {library.albums.length === 0 && !naming && <p className="nav-empty">Make one with +, then drag photos onto it.</p>}
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
    </section>
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
