import { useEffect, useRef } from "react";
import { CloseIcon, FolderIcon } from "../../components/icons";
import type { PreviewFrame } from "../../ipc/frame";
import type { ExportSettings } from "../../ipc/generated/ExportSettings";

/** The dialog's presets (ADR 0050): the design's Web and Social, and the full size. */
export const EXPORT_PRESETS = [
  { id: "web", label: "Web", sub: "JPEG · 2048 px", longEdge: 2048, quality: 85 },
  { id: "social", label: "Social", sub: "JPEG · 1350 px", longEdge: 1350, quality: 90 },
  { id: "full", label: "Full quality", sub: "JPEG · original size", longEdge: null, quality: 95 },
] as const;

const SIZES: ReadonlyArray<{ label: string; longEdge: number | null }> = [
  { label: "Original", longEdge: null },
  { label: "2048 px", longEdge: 2048 },
  { label: "1350 px", longEdge: 1350 },
];

/** Settings changed by hand no longer match a preset. */
function presetOf(longEdge: number | null, quality: number): string | null {
  return EXPORT_PRESETS.find((p) => p.longEdge === longEdge && p.quality === quality)?.id ?? null;
}

/**
 * The design's export dialog: the photos, a preset, JPEG quality and size, the folder,
 * and Export. The choices are remembered (settings); the folder is chosen only in the
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

  const longEdge = settings.longEdge;
  const set = (e: number | null, q: number) => onChange({ longEdge: e, jpegQuality: q, preset: presetOf(e, q) });
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
            <button key={p.id} className="export-preset" aria-pressed={settings.preset === p.id} onClick={() => set(p.longEdge, p.quality)}>
              <span className="export-preset-label">{p.label}</span>
              <span className="export-preset-sub">{p.sub}</span>
            </button>
          ))}
        </div>
        <div className="export-rows">
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
                onChange={(e) => set(longEdge, Number(e.target.value))}
              />
              <span className="export-value">{settings.jpegQuality}</span>
            </div>
          </div>
          <div className="export-row">
            <span>Size</span>
            <div className="segmented small" role="radiogroup" aria-label="Size">
              {SIZES.map((s) => (
                <button key={s.label} role="radio" aria-checked={longEdge === s.longEdge} onClick={() => set(s.longEdge, settings.jpegQuality)}>
                  {s.label}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="export-footer">
          <button className="export-folder" onClick={onChooseFolder} title={settings.folder ?? "Choose where exported photos go"}>
            <span className="export-folder-label">Save to</span>
            <span className="export-folder-name">
              <FolderIcon size={13} />
              {folderName ?? "Choose a folder…"}
            </span>
          </button>
          <div className="export-actions">
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
