import { CloseIcon, EyeIcon, EyeOffIcon, PlusIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { MaskTool } from "./MaskTool";
import { ADDABLE_KINDS, MASK_KINDS, brushSizeFromSlider, maskName, sliderFromBrushSize } from "./masks";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";
import { WHITE_BALANCE_TRACKS } from "./whiteBalance";

const LOCAL_KEYS = ["exposure", "warmth", "clarity"] as const;

/** The brush's settings (ADR 0042): tool settings for the next stroke, not stored
 *  until painted (each stroke keeps its own). */
const BRUSH_SPECS: Record<"size" | "feather" | "flow", AdjustmentSpec> = {
  size: { key: "size", label: "Size", group: "Brush", min: 1, max: 100, step: 1, default: 16, more: false, unit: "" },
  feather: { key: "feather", label: "Feather", group: "Brush", min: 0, max: 100, step: 1, default: 50, more: false, unit: "" },
  flow: { key: "flow", label: "Flow", group: "Brush", min: 1, max: 100, step: 1, default: 100, more: false, unit: "" },
};
const isLocalKey = (k: string): k is keyof LocalAdjustments => (LOCAL_KEYS as readonly string[]).includes(k);

/**
 * The panel's Selective section (ADR 0040), as in the design: the masks (picking one
 * edits it on the photo), the active mask's Exposure, Warmth and Clarity, and the
 * masks to add. Only built kinds are offered.
 */
export function SelectiveControls({
  tool,
  specs,
  feather,
  disabled,
}: {
  tool: MaskTool;
  specs: AdjustmentSpec[];
  /** A radial mask's Feather (ADR 0041). */
  feather: AdjustmentSpec;
  disabled: boolean;
}) {
  const active = tool.active;
  return (
    <div className="selective">
      <div className="mask-list">
        {tool.masks.map((m) => {
          const on = active?.id === m.id;
          return (
            <div key={m.id} className={`mask-row${on ? " active" : ""}${m.hidden ? " hidden" : ""}`}>
              <button className="mask-row-pick" aria-pressed={on} disabled={disabled} onClick={() => tool.pick(m.id)}>
                <span className="mask-swatch">
                  <span className="mask-dot" style={{ background: MASK_KINDS[m.shape.kind].dot }} />
                </span>
                <span className="mask-row-text">
                  <span className="mask-row-name">{maskName(tool.masks, m)}</span>
                  <span className="mask-row-kind">{MASK_KINDS[m.shape.kind].label}</span>
                </span>
              </button>
              <button
                className="mask-row-icon"
                aria-pressed={m.hidden ?? false}
                aria-label={m.hidden ? "Show mask" : "Hide mask"}
                title={m.hidden ? "Show mask" : "Hide mask"}
                disabled={disabled}
                onClick={() => tool.toggleHidden(m.id)}
              >
                {m.hidden ? <EyeOffIcon size={13} /> : <EyeIcon size={13} />}
              </button>
              <button className="mask-row-icon" aria-label="Delete mask" title="Delete mask" disabled={disabled} onClick={() => tool.remove(m.id)}>
                <CloseIcon size={12} />
              </button>
            </div>
          );
        })}
        {tool.masks.length === 0 && (
          <div className="mask-empty">
            Adjust just part of the photo: paint with a brush, darken a sky with a linear gradient, or lift a face with a
            radial one.
          </div>
        )}
      </div>
      {active && (
        <div className="mask-card">
          {specs
            .filter((s) => isLocalKey(s.key))
            .map((spec) => {
              const key = spec.key as keyof LocalAdjustments;
              const value = active.adjustments[key];
              return (
                <Slider
                  key={spec.key}
                  id={`mask-${spec.key}`}
                  spec={spec}
                  value={value}
                  shown={formatSliderValue(value, spec.min, spec.step, spec.unit)}
                  track={key === "warmth" ? WHITE_BALANCE_TRACKS.temperature : undefined}
                  disabled={disabled}
                  onChange={(v) => tool.setAdjustment(key, v)}
                />
              );
            })}
          <div className="mask-shape-controls">
            {active.shape.kind === "brush" && (
              <>
                <div className="brush-mode">
                  <div className="segmented small" role="radiogroup" aria-label="Brush">
                    {([false, true] as const).map((erase) => (
                      <label key={String(erase)}>
                        <input
                          className="sr-only"
                          type="radio"
                          name="brush-mode"
                          checked={tool.brush.erase === erase}
                          disabled={disabled}
                          onChange={() => tool.setBrush({ erase })}
                        />
                        {erase ? "Erase" : "Paint"}
                      </label>
                    ))}
                  </div>
                  <button
                    className="ghost small"
                    disabled={disabled || active.shape.strokes.length === 0}
                    title="Remove everything painted in this mask"
                    onClick={() => tool.setStrokes([])}
                  >
                    Clear
                  </button>
                </div>
                <Slider
                  id="brush-size"
                  spec={BRUSH_SPECS.size}
                  value={sliderFromBrushSize(tool.brush.size)}
                  shown={String(sliderFromBrushSize(tool.brush.size))}
                  zeroMark={false}
                  disabled={disabled}
                  onChange={(v) => tool.setBrush({ size: brushSizeFromSlider(v) })}
                />
                <Slider
                  id="brush-feather"
                  spec={BRUSH_SPECS.feather}
                  value={tool.brush.feather}
                  shown={String(tool.brush.feather)}
                  zeroMark={false}
                  disabled={disabled}
                  onChange={(v) => tool.setBrush({ feather: v })}
                />
                <Slider
                  id="brush-flow"
                  spec={BRUSH_SPECS.flow}
                  value={tool.brush.flow}
                  shown={String(tool.brush.flow)}
                  zeroMark={false}
                  disabled={disabled}
                  onChange={(v) => tool.setBrush({ flow: v })}
                />
                <p className="brush-hint">Paint on the photo. Hold Option to erase; [ and ] change the size.</p>
              </>
            )}
            {active.shape.kind === "radial" && (
              <Slider
                id="mask-feather"
                spec={feather}
                value={active.shape.feather}
                shown={formatSliderValue(active.shape.feather, feather.min, feather.step, feather.unit)}
                zeroMark={false}
                disabled={disabled}
                onChange={tool.setFeather}
              />
            )}
            <button
              className="lens-toggle"
              aria-pressed={active.invert ?? false}
              disabled={disabled}
              onClick={() => tool.setInvert(!(active.invert ?? false))}
            >
              <span className="lens-toggle-text">
                <span className="lens-toggle-label">Invert</span>
                <span className="lens-toggle-sub">Adjust outside the mask instead</span>
              </span>
              <span className="switch" aria-hidden="true">
                <span className="switch-knob" />
              </span>
            </button>
          </div>
        </div>
      )}
      <div className="mask-add-grid">
        {ADDABLE_KINDS.map((k) => (
          <button key={k} className="chip mask-add-tile" title={MASK_KINDS[k].hint} disabled={disabled} onClick={() => tool.add(k)}>
            <PlusIcon size={14} />
            {MASK_KINDS[k].add}
          </button>
        ))}
      </div>
    </div>
  );
}
