import type { WatermarkPosition } from "../../ipc/generated/WatermarkPosition";
import type { WatermarkSize } from "../../ipc/generated/WatermarkSize";
import { estimateExport } from "../../ipc/client";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { ExportEstimateDto } from "../../ipc/generated/ExportEstimateDto";
import type { ExportEstimateRequestDto } from "../../ipc/generated/ExportEstimateRequestDto";
import { formatEstimate } from "./estimate";
import { useEffect, useRef, useState } from "react";
import { CloseIcon, FolderIcon } from "../../components/icons";
import type { PreviewFrame } from "../../ipc/frame";
import type { ExportColourSpace } from "../../ipc/generated/ExportColourSpace";
import type { ExportFileFormat } from "../../ipc/generated/ExportFileFormat";
import type { ExportSettings } from "../../ipc/generated/ExportSettings";
import type { OutputSharpening } from "../../ipc/generated/OutputSharpening";

/** The settings a preset sets. */
type Choice = {
  format: ExportFileFormat;
  longEdge: number | null;
  jpegQuality: number;
  colourSpace: ExportColourSpace;
  sharpen: OutputSharpening;
};

/** The dialog's presets (ADRs 0050, 0057, 0059, 0062), as the design has them: Web and
 *  Social (sRGB JPEGs) and Full quality (a 16-bit Adobe RGB TIFF at the original size),
 *  each sharpened for the screen. */
export const EXPORT_PRESETS: ReadonlyArray<{ id: string; label: string; sub: string } & Choice> = [
  { id: "web", label: "Web", sub: "JPEG · 2048 px", format: "jpeg", longEdge: 2048, jpegQuality: 85, colourSpace: "srgb", sharpen: "screen" },
  { id: "social", label: "Social", sub: "JPEG · 1350 px", format: "jpeg", longEdge: 1350, jpegQuality: 90, colourSpace: "srgb", sharpen: "screen" },
  { id: "full", label: "Full quality", sub: "TIFF · original", format: "tiff", longEdge: null, jpegQuality: 95, colourSpace: "adobeRgb", sharpen: "screen" },
];

/** Where the watermark sits (ADR 0069), and how large it is. */
const WATERMARK_POSITIONS: ReadonlyArray<{ id: WatermarkPosition; label: string }> = [
  { id: "topLeft", label: "Top left" },
  { id: "topRight", label: "Top right" },
  { id: "centre", label: "Centre" },
  { id: "bottomLeft", label: "Bottom left" },
  { id: "bottomRight", label: "Bottom right" },
];
const WATERMARK_SIZES: ReadonlyArray<{ id: WatermarkSize; short: string; label: string }> = [
  { id: "small", short: "S", label: "Small" },
  { id: "medium", short: "M", label: "Medium" },
  { id: "large", short: "L", label: "Large" },
];

/** A frame with a dot where the watermark sits. */
function PositionIcon({ at }: { at: WatermarkPosition }) {
  const [x, y] = {
    topLeft: [4.5, 4.5],
    topRight: [11.5, 4.5],
    centre: [8, 8],
    bottomLeft: [4.5, 11.5],
    bottomRight: [11.5, 11.5],
  }[at];
  return (
    <svg className="icon" width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <rect x="1.5" y="1.5" width="13" height="13" rx="2" />
      <circle cx={x} cy={y} r="1.6" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** Changes to the choices wait this long before the size is estimated again. */
const ESTIMATE_DELAY_MS = 150;

/** The design's Colour space choices (ADR 0062). The photo is edited in sRGB (with
 *  colours beyond it brought in softly), so the wider spaces hold the same colours,
 *  tagged as their space for displays, labs and workflows that ask for it. */
const COLOUR_SPACES: ReadonlyArray<{ id: ExportColourSpace; label: string; hint: string }> = [
  { id: "srgb", label: "sRGB", hint: "For the web, phones and most screens" },
  { id: "displayP3", label: "Display P3", hint: "Tagged for wide-gamut screens (the colours are sRGB's)" },
  { id: "adobeRgb", label: "Adobe RGB", hint: "Tagged for print labs and print workflows (the colours are sRGB's)" },
];

/** The design's Sharpen for choices, and None for files that will be edited further. */
const SHARPENING: ReadonlyArray<{ id: OutputSharpening; label: string; hint: string }> = [
  { id: "none", label: "None", hint: "No output sharpening: for further editing" },
  { id: "screen", label: "Screen", hint: "Light and fine, for viewing at this size" },
  { id: "matte", label: "Matte", hint: "For printing on matte paper (300 ppi)" },
  { id: "glossy", label: "Glossy", hint: "For printing on glossy paper (300 ppi)" },
];

/** The design's Format choices; HEIC is not offered (ADR 0057). */
const FORMATS: ReadonlyArray<{ id: ExportFileFormat; label: string; hint: string }> = [
  { id: "jpeg", label: "JPEG", hint: "Small files for sharing and the web" },
  { id: "tiff", label: "TIFF", hint: "16-bit, lossless: for printing and further editing" },
  { id: "png", label: "PNG", hint: "8-bit, lossless" },
];

const SIZES: ReadonlyArray<{ label: string; longEdge: number | null }> = [
  { label: "Original", longEdge: null },
  { label: "2048 px", longEdge: 2048 },
  { label: "1350 px", longEdge: 1350 },
];

/** Settings changed by hand no longer match a preset (quality counts for JPEG only). */
function presetOf(c: Choice): string | null {
  const matches = (p: Choice) =>
    p.format === c.format &&
    p.longEdge === c.longEdge &&
    p.sharpen === c.sharpen &&
    p.colourSpace === c.colourSpace &&
    (c.format !== "jpeg" || p.jpegQuality === c.jpegQuality);
  return EXPORT_PRESETS.find(matches)?.id ?? null;
}

/**
 * The design's export dialog: the photos, a preset, the format, JPEG quality, size,
 * colour space and output sharpening, what metadata to keep, the folder, and Export. The choices are remembered (settings); the folder is chosen only in the
 * system's dialog.
 */
export function ExportDialog({
  count,
  names,
  frame,
  settings,
  onChange,
  onChooseFolder,
  onExport,
  onClose,
  photo,
}: {
  count: number;
  /** The first photo's name, for "DSC_0012.NEF and 11 others". */
  names: string;
  /** The open photo as shown, for the header's picture. */
  frame: PreviewFrame | null;
  settings: ExportSettings;
  onChange: (change: Partial<ExportSettings>) => void;
  onChooseFolder: () => void;
  onExport: () => void;
  onClose: () => void;
  /** The open photo and its edit, for the size estimate (ADR 0068); null without one. */
  photo: { imageId: number; recipe: EditRecipe } | null;
}) {
  const thumb = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = thumb.current;
    if (!c || !frame) return;
    c.width = frame.width;
    c.height = frame.height;
    c.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  }, [frame]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // The size estimate (ADR 0068): asked for when the choices settle, the newest wins.
  const [estimate, setEstimate] = useState<ExportEstimateDto | null>(null);
  const estimateFor = photo
    ? {
        imageId: photo.imageId,
        recipe: photo.recipe,
        format: settings.format,
        quality: settings.jpegQuality,
        longEdge: settings.longEdge ?? undefined,
        sharpen: settings.sharpen,
        colourSpace: settings.colourSpace,
      }
    : null;
  const estimateKey = estimateFor ? JSON.stringify(estimateFor) : null;
  useEffect(() => {
    if (!estimateKey) {
      setEstimate(null);
      return;
    }
    let current = true;
    const timer = window.setTimeout(() => {
      estimateExport(JSON.parse(estimateKey) as ExportEstimateRequestDto).then(
        (e) => current && setEstimate(e),
        () => undefined, // replaced by a newer one, or no estimate: show the last
      );
    }, ESTIMATE_DELAY_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [estimateKey]);

  const longEdge = settings.longEdge;
  const format = settings.format;
  const current: Choice = {
    format,
    longEdge,
    jpegQuality: settings.jpegQuality,
    colourSpace: settings.colourSpace,
    sharpen: settings.sharpen,
  };
  const set = (change: Partial<Choice>) => {
    const next = { ...current, ...change };
    onChange({ ...next, preset: presetOf(next) });
  };
  const folderName = settings.folder?.split(/[\\/]/).filter(Boolean).pop() ?? null;
  const title = count === 1 ? "Export photo" : `Export ${count} photos`;

  return (
    <div className="modal-backdrop export-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="export-dialog" role="dialog" aria-modal="true" aria-labelledby="export-title">
        <div className="export-header">
          <div className="export-heading">
            <canvas ref={thumb} className="export-thumb" aria-hidden="true" />
            <div className="export-titles">
              <h2 id="export-title">{title}</h2>
              <span className="export-sub">{names}</span>
            </div>
          </div>
          <button className="icon-button export-close" aria-label="Close" onClick={onClose}>
            <CloseIcon size={14} />
          </button>
        </div>
        <div className="export-presets">
          {EXPORT_PRESETS.map((p) => (
            <button key={p.id} className="export-preset" aria-pressed={settings.preset === p.id} onClick={() => set({ format: p.format, longEdge: p.longEdge, jpegQuality: p.jpegQuality, colourSpace: p.colourSpace, sharpen: p.sharpen })}>
              <span className="export-preset-label">{p.label}</span>
              <span className="export-preset-sub">{p.sub}</span>
            </button>
          ))}
        </div>
        <div className="export-rows">
          <div className="export-row">
            <span>Format</span>
            <div className="segmented small" role="radiogroup" aria-label="Format">
              {FORMATS.map((f) => (
                <button key={f.id} role="radio" aria-checked={format === f.id} title={f.hint} onClick={() => set({ format: f.id })}>
                  {f.label}
                </button>
              ))}
            </div>
          </div>
          {/* Quality is JPEG's alone; TIFF and PNG are lossless. */}
          {format === "jpeg" && (
            <div className="export-row">
              <label htmlFor="export-quality">Quality</label>
              <div className="export-control">
                <input
                  id="export-quality"
                  className="range export-range"
                  type="range"
                  min={50}
                  max={100}
                  step={1}
                  value={settings.jpegQuality}
                  onChange={(e) => set({ jpegQuality: Number(e.target.value) })}
                />
                <span className="export-value">{settings.jpegQuality}</span>
              </div>
            </div>
          )}
          <div className="export-row">
            <span>Size</span>
            <div className="segmented small" role="radiogroup" aria-label="Size">
              {SIZES.map((s) => (
                <button key={s.label} role="radio" aria-checked={longEdge === s.longEdge} onClick={() => set({ longEdge: s.longEdge })}>
                  {s.label}
                </button>
              ))}
            </div>
          </div>
          <div className="export-row">
            <span>Colour space</span>
            <div className="segmented small" role="radiogroup" aria-label="Colour space">
              {COLOUR_SPACES.map((o) => (
                <button key={o.id} role="radio" aria-checked={settings.colourSpace === o.id} title={o.hint} onClick={() => set({ colourSpace: o.id })}>
                  {o.label}
                </button>
              ))}
            </div>
          </div>
          <div className="export-row">
            <span>Sharpen for</span>
            <div className="segmented small" role="radiogroup" aria-label="Sharpen for">
              {SHARPENING.map((o) => (
                <button key={o.id} role="radio" aria-checked={settings.sharpen === o.id} title={o.hint} onClick={() => set({ sharpen: o.id })}>
                  {o.label}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="export-toggles">
          <ExportSwitch
            label="Keep metadata"
            hint="Camera, lens, exposure and when it was taken"
            on={settings.keepMetadata}
            onChange={(keepMetadata) => onChange({ keepMetadata })}
          />
          <ExportSwitch
            label="Strip location"
            hint={settings.keepMetadata ? "Leave out where it was taken" : "No metadata is kept, so no location either"}
            on={settings.stripLocation}
            disabled={!settings.keepMetadata}
            onChange={(stripLocation) => onChange({ stripLocation })}
          />
          <ExportSwitch
            label="Watermark"
            hint="A line of text in a corner of each photo"
            on={settings.watermark.enabled}
            onChange={(enabled) => onChange({ watermark: { ...settings.watermark, enabled } })}
          />
        </div>
        {settings.watermark.enabled && (
          <div className="export-watermark">
            <input
              className="text-input"
              aria-label="Watermark text"
              placeholder={`© ${new Date().getFullYear()} Your name`}
              maxLength={120}
              value={settings.watermark.text}
              onChange={(e) => onChange({ watermark: { ...settings.watermark, text: e.target.value } })}
            />
            <div className="segmented small" role="radiogroup" aria-label="Watermark position">
              {WATERMARK_POSITIONS.map((p) => (
                <button
                  key={p.id}
                  role="radio"
                  aria-checked={settings.watermark.position === p.id}
                  aria-label={p.label}
                  title={p.label}
                  onClick={() => onChange({ watermark: { ...settings.watermark, position: p.id } })}
                >
                  <PositionIcon at={p.id} />
                </button>
              ))}
            </div>
            <div className="segmented small" role="radiogroup" aria-label="Watermark size">
              {WATERMARK_SIZES.map((s) => (
                <button
                  key={s.id}
                  role="radio"
                  aria-checked={settings.watermark.size === s.id}
                  title={s.label}
                  onClick={() => onChange({ watermark: { ...settings.watermark, size: s.id } })}
                >
                  {s.short}
                </button>
              ))}
            </div>
          </div>
        )}
        <div className="export-footer">
          <button className="export-folder" onClick={onChooseFolder} title={settings.folder ?? "Choose where exported photos go"}>
            <span className="export-folder-label">Save to</span>
            <span className="export-folder-name">
              <FolderIcon size={13} />
              {folderName ?? "Choose a folder…"}
            </span>
          </button>
          <div className="export-actions">
            {estimate && (
              <span className="export-estimate" title={`Estimated from a preview of this photo: ${estimate.width} × ${estimate.height} px`}>
                {formatEstimate(estimate.bytes, count)}
              </span>
            )}
            <button className="ghost" onClick={onClose}>
              Cancel
            </button>
            <button className="primary export-go" autoFocus onClick={onExport}>
              {settings.folder ? (count === 1 ? "Export photo" : `Export ${count} photos`) : "Choose folder and export"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/** One of the design's switch tiles under the rows (ADR 0063). They are not part of
 *  the presets, which leave them as they are. */
function ExportSwitch({
  label,
  hint,
  on,
  disabled = false,
  onChange,
}: {
  label: string;
  hint: string;
  on: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button className="export-switch" role="switch" aria-checked={on} disabled={disabled} title={hint} onClick={() => onChange(!on)}>
      <span>{label}</span>
      <span className="switch" aria-hidden="true">
        <span className="switch-knob" />
      </span>
    </button>
  );
}
