import { useEffect } from "react";
import { FolderIcon, ImportIcon, PhotosIcon, RefreshIcon, StarIcon } from "../../components/icons";
import * as ipc from "../../ipc/client";
import { formatBytes, formatCaptured, formatDateTime } from "../../lib/format";
import type { SettingsApi } from "../settings/useSettings";
import { indexStatusText } from "./indexStatus";
import { PhotoThumbnail } from "./PhotoThumbnail";
import type { LibraryApi } from "./useLibrary";

interface Props {
  library: LibraryApi;
  settings: SettingsApi;
  onOpenPhoto: (path: string) => void;
}

let defaultFolderTried = false;

const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/** Library mode: browse granted folders and open a photo. */
export function LibraryView({ library, settings, onOpenPhoto }: Props) {
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

  const recent = s?.library.recentFolders ?? [];
  const currentRoot = listing?.breadcrumbs[0]?.path;
  const photoCount = listing ? `${listing.photos.length} photo${listing.photos.length === 1 ? "" : "s"}` : "";

  return (
    <div className="library-view">
      <aside className="sidebar-left">
        <div className="sidebar-scroll scroll">
          <section className="nav-section" aria-label="Library">
            <div className="nav-label">Library</div>
            <div className="nav-row">
              <PhotosIcon />
              <span className="grow">Indexed photos</span>
              <span className="count">{library.status ? library.status.photos.toLocaleString() : "—"}</span>
            </div>
          </section>
          <section className="nav-section" aria-label="Folders">
            <div className="nav-label">
              Folders
              <button className="icon-button" aria-label="Add folder" title="Add folder" onClick={() => void choose()}>
                <svg className="icon" width="13" height="13" viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M8 3v10M3 8h10" />
                </svg>
              </button>
            </div>
            {recent.length === 0 && <p className="nav-empty">No folders yet.</p>}
            {recent.map((f) => (
              <button
                key={f}
                className="nav-row"
                aria-current={currentRoot === f ? "true" : undefined}
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
          </section>
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
        {!listing ? (
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
                {listing.breadcrumbs.length > 1 && (
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
                <div className="page-title-row">
                  <h1>{listing.name}</h1>
                  <span className="subtle">
                    {photoCount}
                    {loading ? " · loading…" : ""}
                  </span>
                </div>
              </div>
              <div className="page-actions">
                {indexText && <span className="index-status">{indexText}</span>}
                <button className="ghost" onClick={() => void library.refresh()}>
                  <RefreshIcon />
                  Refresh
                </button>
                {listing.path !== defaultFolder && (
                  <button className="ghost" onClick={() => void makeDefault()}>
                    Set as default
                  </button>
                )}
              </div>
            </header>

            <div className="library-scroll scroll">
              {listing.folders.length > 0 && (
                <div className="chips">
                  {listing.folders.map((f) => (
                    <button key={f.path} className="chip" onClick={() => void library.openFolder(f.path)}>
                      <FolderIcon size={14} />
                      {f.name}
                    </button>
                  ))}
                </div>
              )}
              {listing.photos.length === 0 ? (
                <p className="muted">No supported photos in this folder.</p>
              ) : (
                <table className="photo-table">
                  <thead>
                    <tr>
                      <th aria-label="Thumbnail" />
                      <th>Name</th>
                      <th>Taken</th>
                      <th>Camera</th>
                      <th>Type</th>
                      <th className="num">Size</th>
                    </tr>
                  </thead>
                  <tbody>
                    {listing.photos.map((p) => (
                      <tr key={p.path}>
                        <td className="thumb-cell">
                          <button className="thumb-button" onClick={() => onOpenPhoto(p.path)} aria-label={`Open ${p.name} in Edit`}>
                            <PhotoThumbnail path={p.path} />
                          </button>
                        </td>
                        <td>
                          <button className="text-button name" onClick={() => onOpenPhoto(p.path)} title="Open in Edit">
                            {p.name}
                          </button>
                        </td>
                        <td title={p.details?.capturedAt ? "Capture time recorded by the camera" : "File date (not indexed yet)"}>
                          {formatCaptured(p.details?.capturedAt) ?? <span className="file-date">{formatDateTime(p.modifiedMs)}</span>}
                        </td>
                        <td title={p.details?.lens ?? undefined}>{p.details?.camera ?? <span className="muted">—</span>}</td>
                        <td className="type">{p.raw ? "RAW" : "JPEG"}</td>
                        <td className="num">{formatBytes(p.sizeBytes)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
              {listing.skipped > 0 && <p className="muted">{listing.skipped} item(s) couldn’t be read and are not shown.</p>}
            </div>
          </>
        )}
      </section>
    </div>
  );
}
