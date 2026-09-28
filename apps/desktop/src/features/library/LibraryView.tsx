import { useEffect } from "react";
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

  return (
    <div className="library-view">
      <aside className="library-sidebar">
        <button className="primary" onClick={() => void choose()}>
          Choose folder…
        </button>
        {library.status && library.status.photos > 0 && (
          <p className="muted library-total">{library.status.photos.toLocaleString()} photos in library</p>
        )}
        {s && s.library.recentFolders.length > 0 && (
          <>
            <h3>Recent</h3>
            <ul className="recent">
              {s.library.recentFolders.map((f) => (
                <li key={f}>
                  <button
                    className={listing?.breadcrumbs[0]?.path === f ? "active" : undefined}
                    title={f}
                    onClick={() => void library.openFolder(f)}
                  >
                    {f === defaultFolder ? "★ " : ""}
                    {f.split(/[\\/]/).filter(Boolean).pop() ?? f}
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
      </aside>

      <section className="library-main">
        {library.status?.notice && <p className="notice library-notice">{library.status.notice}</p>}
        {!listing ? (
          <div className="empty">
            <p>{loading ? "Loading…" : "Choose a folder of photos to browse."}</p>
            {!loading && (
              <button className="primary" onClick={() => void choose()}>
                Choose folder…
              </button>
            )}
          </div>
        ) : (
          <>
            <div className="mode-toolbar">
              <nav className="crumbs" aria-label="Folder">
                {listing.breadcrumbs.map((c, i) => (
                  <span key={c.path}>
                    {i > 0 && <span className="sep">›</span>}
                    <button onClick={() => void library.openFolder(c.path)} disabled={i === listing.breadcrumbs.length - 1}>
                      {c.name}
                    </button>
                  </span>
                ))}
              </nav>
              <span className="muted">
                {listing.photos.length} photo{listing.photos.length === 1 ? "" : "s"} here
                {loading ? " · loading…" : ""}
              </span>
              {indexText && <span className="index-status">{indexText}</span>}
              <span className="spacer" />
              <button onClick={() => void library.refresh()}>Refresh</button>
              {listing.path !== defaultFolder && <button onClick={() => void makeDefault()}>Set as default</button>}
            </div>

            <div className="library-list">
              {listing.folders.length > 0 && (
                <ul className="folders">
                  {listing.folders.map((f) => (
                    <li key={f.path}>
                      <button onClick={() => void library.openFolder(f.path)}>📁 {f.name}</button>
                    </li>
                  ))}
                </ul>
              )}
              {listing.photos.length === 0 ? (
                <p className="muted">No supported photos in this folder.</p>
              ) : (
                <table className="photos">
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
                          <button className="link" onClick={() => onOpenPhoto(p.path)} title="Open in Edit">
                            {p.name}
                          </button>
                        </td>
                        <td title={p.details?.capturedAt ? "Capture time recorded by the camera" : "File date (not indexed yet)"}>
                          {formatCaptured(p.details?.capturedAt) ?? <span className="muted">{formatDateTime(p.modifiedMs)}</span>}
                        </td>
                        <td title={p.details?.lens ?? undefined}>{p.details?.camera ?? <span className="muted">—</span>}</td>
                        <td>{p.raw ? "RAW" : "JPEG"}</td>
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
