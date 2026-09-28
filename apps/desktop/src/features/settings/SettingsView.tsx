import type { BackgroundIntensity } from "../../ipc/generated/BackgroundIntensity";
import type { Theme } from "../../ipc/generated/Theme";
import type { SettingsApi } from "./useSettings";

const THEMES: Array<[Theme, string]> = [
  ["system", "Match system"],
  ["light", "Light"],
  ["dark", "Dark"],
];

const INTENSITY: Array<[BackgroundIntensity, string, string]> = [
  ["low", "Low", "Exports take longer; editing stays smoothest."],
  ["balanced", "Balanced", "Recommended for most computers."],
  ["high", "High", "Fastest exports; editing may feel slower while exporting."],
];

const CACHE_SIZES_MB = [128, 256, 512, 1024, 2048];

/** Settings that change behaviour today. Values are validated and stored by Rust. */
export function SettingsView({ api }: { api: SettingsApi }) {
  const { view, update } = api;
  if (!view) return <p className="muted">Loading settings…</p>;
  const s = view.settings;
  return (
    <div className="settings">
      {view.recoveredFrom && (
        <p className="notice">
          Your settings file couldn’t be read, so settings were reset to their defaults. The old file was kept at{" "}
          <code>{view.recoveredFrom}</code>.
        </p>
      )}

      <section>
        <h2>General</h2>
        <fieldset className="segmented" aria-label="Theme">
          <legend>Theme</legend>
          {THEMES.map(([value, label]) => (
            <label key={value}>
              <input
                type="radio"
                name="theme"
                checked={s.general.theme === value}
                onChange={() => update((x) => ({ ...x, general: { ...x.general, theme: value } }))}
              />
              {label}
            </label>
          ))}
        </fieldset>
      </section>

      <section>
        <h2>Performance</h2>
        <label className="field">
          <span>Preview memory</span>
          <select
            value={s.performance.previewCacheMb}
            onChange={(e) => {
              const mb = Number(e.currentTarget.value);
              update((x) => ({ ...x, performance: { ...x.performance, previewCacheMb: mb } }));
            }}
          >
            {CACHE_SIZES_MB.map((mb) => (
              <option key={mb} value={mb}>
                {mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`}
              </option>
            ))}
          </select>
          <small>Memory used to keep recent previews for instant switching. Applies immediately.</small>
        </label>
        <fieldset className="choices" aria-label="Background processing">
          <legend>Background processing</legend>
          {INTENSITY.map(([value, label, help]) => (
            <label key={value}>
              <input
                type="radio"
                name="intensity"
                checked={s.performance.backgroundIntensity === value}
                onChange={() =>
                  update((x) => ({ ...x, performance: { ...x.performance, backgroundIntensity: value } }))
                }
              />
              <span>
                {label} <small>{help}</small>
              </span>
            </label>
          ))}
          {view.restartRequired && <p className="notice">Restart the app to apply the new background setting.</p>}
        </fieldset>
      </section>

      <section>
        <h2>Export</h2>
        <label className="field">
          <span>
            JPEG quality <output>{s.export.jpegQuality}</output>
          </span>
          <input
            type="range"
            min={50}
            max={100}
            value={s.export.jpegQuality}
            onChange={(e) => {
              const q = Number(e.currentTarget.value);
              update((x) => ({ ...x, export: { ...x.export, jpegQuality: q } }));
            }}
          />
          <small>Higher keeps more detail and makes larger files. 90–95 suits most photos.</small>
        </label>
      </section>

      <section>
        <h2>Library</h2>
        <div className="field">
          <span>Default folder</span>
          <p className="muted">{s.library.defaultFolder ?? "None. Choose one in Library with “Set as default”."}</p>
          {s.library.defaultFolder && (
            <button onClick={() => update((x) => ({ ...x, library: { ...x.library, defaultFolder: null } }))}>
              Clear default folder
            </button>
          )}
        </div>
      </section>
    </div>
  );
}
