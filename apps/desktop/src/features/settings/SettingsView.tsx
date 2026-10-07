import type { BackgroundIntensity } from "../../ipc/generated/BackgroundIntensity";
import type { Theme } from "../../ipc/generated/Theme";
import { sliderTrack } from "../editor/sliderTrack";
import { BackupSettings } from "./BackupSettings";
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
  if (!view) return <p className="empty-state">Loading settings…</p>;
  const s = view.settings;
  return (
    <div className="settings-page scroll">
      <div className="settings">
        <h1>Settings</h1>
        {view.recoveredFrom && (
          <p className="notice">
            Your settings file couldn’t be read, so settings were reset to their defaults. The old file was kept at{" "}
            <code>{view.recoveredFrom}</code>.
          </p>
        )}

        <section className="settings-group">
          <h2>General</h2>
          <div className="settings-card">
            <div className="setting">
              <div className="setting-text">
                <span className="setting-name" id="theme-label">
                  Appearance
                </span>
                <small>Dark suits photo work; light follows the same layout.</small>
              </div>
              <div className="segmented small" role="radiogroup" aria-labelledby="theme-label">
                {THEMES.map(([value, label]) => (
                  <label key={value}>
                    <input
                      className="sr-only"
                      type="radio"
                      name="theme"
                      checked={s.general.theme === value}
                      onChange={() => update((x) => ({ ...x, general: { ...x.general, theme: value } }))}
                    />
                    {label}
                  </label>
                ))}
              </div>
            </div>
          </div>
        </section>

        <section className="settings-group">
          <h2>Performance</h2>
          <div className="settings-card">
            <label className="setting">
              <span className="setting-text">
                <span className="setting-name">Preview memory</span>
                <small>Memory used to keep recent previews for instant switching. Applies immediately.</small>
              </span>
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
            </label>
            <fieldset className="setting stacked" aria-label="Background processing">
              <legend className="sr-only">Background processing</legend>
              <div className="setting-text">
                <span className="setting-name">Background processing</span>
                <small>How much of the computer exports may use while you keep editing.</small>
              </div>
              <div className="choice-list">
                {INTENSITY.map(([value, label, help]) => (
                  <label key={value} className="choice">
                    <input
                      type="radio"
                      name="intensity"
                      checked={s.performance.backgroundIntensity === value}
                      onChange={() =>
                        update((x) => ({ ...x, performance: { ...x.performance, backgroundIntensity: value } }))
                      }
                    />
                    <span>
                      {label}
                      <small>{help}</small>
                    </span>
                  </label>
                ))}
              </div>
              {view.restartRequired && <p className="notice">Restart the app to apply the new background setting.</p>}
            </fieldset>
          </div>
        </section>

        <section className="settings-group">
          <h2>Export</h2>
          <div className="settings-card">
            <div className="setting stacked">
              <div className="slider-head">
                <label className="setting-text" htmlFor="jpeg-quality">
                  <span className="setting-name">JPEG quality</span>
                  <small>Higher keeps more detail and makes larger files. 90–95 suits most photos.</small>
                </label>
                <output className="setting-value" htmlFor="jpeg-quality">
                  {s.export.jpegQuality}
                </output>
              </div>
              <input
                id="jpeg-quality"
                className="range"
                type="range"
                min={50}
                max={100}
                value={s.export.jpegQuality}
                style={{ background: sliderTrack(s.export.jpegQuality, 50, 100).background }}
                onChange={(e) => {
                  const q = Number(e.currentTarget.value);
                  update((x) => ({ ...x, export: { ...x.export, jpegQuality: q } }));
                }}
              />
            </div>
          </div>
        </section>

        <BackupSettings />

        <section className="settings-group">
          <h2>Library</h2>
          <div className="settings-card">
            <div className="setting">
              <div className="setting-text">
                <span className="setting-name">Default folder</span>
                <small>{s.library.defaultFolder ?? "None. Choose one in Library with “Set as default”."}</small>
              </div>
              {s.library.defaultFolder && (
                <button onClick={() => update((x) => ({ ...x, library: { ...x.library, defaultFolder: null } }))}>
                  Clear
                </button>
              )}
            </div>
            <div className="setting">
              <div className="setting-text">
                <span className="setting-name" id="sidecars-label">
                  Sidecar files for other apps
                </span>
                <small>
                  Writes star ratings and colour labels to an .xmp file beside each RAW, where Lightroom, Bridge and Capture One
                  read them. Your RAW files are never changed; existing sidecars keep everything else in them.
                </small>
              </div>
              <button
                className="setting-switch"
                role="switch"
                aria-checked={s.library.writeSidecars}
                aria-labelledby="sidecars-label"
                onClick={() => update((x) => ({ ...x, library: { ...x.library, writeSidecars: !x.library.writeSidecars } }))}
              >
                <span className="switch" aria-hidden="true">
                  <span className="switch-knob" />
                </span>
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
