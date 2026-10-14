import { useEffect, useMemo, useState } from "react";
import { ChevronIcon, CloseIcon, SearchIcon } from "../../components/icons";
import { filterNotices, loadNotices, type NoticeEntry, type Notices } from "./notices";

/** Settings → About → Third-party notices (ADR 0087): each library and its licence texts. */
export function ThirdPartyNotices({ onBack }: { onBack: () => void }) {
  const [notices, setNotices] = useState<Notices | null | undefined>(undefined);
  const [query, setQuery] = useState("");
  useEffect(() => {
    let live = true;
    void loadNotices().then((n) => live && setNotices(n));
    return () => {
      live = false;
    };
  }, []);
  const groups = useMemo(() => filterNotices(notices?.groups ?? [], query), [notices, query]);

  return (
    <div className="settings-page scroll">
      <div className="settings notices">
        <button className="ghost notices-back" onClick={onBack}>
          <span className="notices-back-icon" aria-hidden="true">
            <ChevronIcon size={13} />
          </span>
          Settings
        </button>
        <h1>Third-party notices</h1>
        <p className="notices-intro">
          This app is built with the open-source software below, each used under the licence shown. Select one to read its
          licence and copyright notices.
        </p>
        {notices === undefined && <p className="notices-intro">Loading…</p>}
        {notices === null && (
          <p className="notice">The notices are written when the app is built, so this development build has none.</p>
        )}
        {notices && (
          <>
            <label className="search-field">
              <SearchIcon size={15} />
              <input
                type="search"
                placeholder="Filter by name or licence"
                aria-label="Filter notices"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Escape" && setQuery("")}
              />
              {query && (
                <button className="search-clear" aria-label="Clear filter" title="Clear filter" onClick={() => setQuery("")}>
                  <CloseIcon size={11} />
                </button>
              )}
            </label>
            {groups.length === 0 && <p className="notices-intro">Nothing matches “{query.trim()}”.</p>}
            {groups.map((g) => (
              <section key={g.title} className="settings-group">
                <h2>
                  {g.title} · {g.entries.length}
                </h2>
                <div className="settings-card">
                  {g.entries.map((e) => (
                    <Notice key={`${e.name}@${e.version}`} entry={e} texts={notices.texts} />
                  ))}
                </div>
              </section>
            ))}
          </>
        )}
      </div>
    </div>
  );
}

/** One library; its texts are only laid out once it is opened. */
function Notice({ entry, texts }: { entry: NoticeEntry; texts: Record<string, string> }) {
  const [open, setOpen] = useState(false);
  return (
    <details className="setting notice-entry" onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>
        <span className="setting-name">{entry.name}</span>
        <span className="notice-version">{entry.version}</span>
        <span className="notice-licence">{entry.licence}</span>
      </summary>
      {open && (
        <div className="notice-body">
          {entry.note && <p className="notice">{entry.note}</p>}
          <p className="notice-source">
            Source: {entry.source}
          </p>
          {entry.texts.map((id) => (
            <pre key={id} className="notice-text">
              {texts[id]}
            </pre>
          ))}
        </div>
      )}
    </details>
  );
}
