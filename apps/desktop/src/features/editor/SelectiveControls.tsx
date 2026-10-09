import { CloseIcon, EyeIcon, EyeOffIcon, PlusIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { MaskTool } from "./MaskTool";
import type { Combine } from "../../ipc/generated/Combine";
import { COMBINE_MODES, FINDING, MASK_KINDS, brushSizeFromSlider, lookOf, maskName, sliderFromBrushSize } from "./masks";
import type { MaskShape } from "../../ipc/generated/MaskShape";
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
const MODES: readonly Combine[] = ["add", "subtract", "intersect"];

/**
 * The panel's Selective section (ADR 0040), as in the design: the masks (picking one
 * edits it on the photo), the active mask's Exposure, Warmth and Clarity, and the
 * masks to add. Only built kinds are offered, and Subject and People (ADR 0074) only
 * where this computer can make them. The card also holds the mask's Density and its
 * shapes (ADR 0043): more can be added to it or subtracted from it.
 */

/** A shape's kind in lists; generated ones say so, as in the design. */
const kindLabel = (shape: MaskShape) => MASK_KINDS[lookOf(shape)].label + (shape.kind === "generated" ? " · detected" : "");
export function SelectiveControls({
  tool,
  specs,
  feather,
  density,
  disabled,
}: {
  tool: MaskTool;
  specs: AdjustmentSpec[];
  /** A radial mask's Feather (ADR 0041). */
  feather: AdjustmentSpec;
  /** A mask's Density (ADR 0043). */
  density: AdjustmentSpec;
  disabled: boolean;
}) {
  const active = tool.active;
  const shape = tool.shape;
  return (
    <div className="selective">
      <div className="mask-list">
        {tool.masks.map((m) => {
          const on = active?.id === m.id;
          return (
            <div key={m.id} className={`mask-row${on ? " active" : ""}${m.hidden ? " hidden" : ""}`}>
              <button className="mask-row-pick" aria-pressed={on} disabled={disabled} onClick={() => tool.pick(m.id)}>
                <span className="mask-swatch">
                  <span className="mask-dot" style={{ background: MASK_KINDS[lookOf(m.shape)].dot }} />
                </span>
                <span className="mask-row-text">
                  <span className="mask-row-name">{maskName(tool.masks, m)}</span>
                  <span className="mask-row-kind">
                    {kindLabel(m.shape)}
                    {m.parts?.length ? ` + ${m.parts.length} more` : ""}
                  </span>
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
            Adjust just part of the photo: {tool.addable.includes("subject") && "pick out the subject, "}paint with a brush,
            darken a sky with a linear gradient, or lift a face with a radial one.
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
          <Slider
            id="mask-density"
            spec={density}
            value={active.density ?? 100}
            shown={formatSliderValue(active.density ?? 100, density.min, density.step, density.unit)}
            zeroMark={false}
            disabled={disabled}
            onChange={tool.setDensity}
          />
          <div className="mask-shapes">
            {tool.shapes.length > 1 && (
              <div className="mask-shape-list">
                {tool.shapes.map(({ shape: s, mode }, i) => {
                  const on = i === tool.shapeIndex;
                  return (
                    <div key={i} className={on ? "mask-shape-row active" : "mask-shape-row"}>
                      <button className="mask-shape-pick" aria-pressed={on} disabled={disabled} onClick={() => tool.pickShape(i)}>
                        <span className="mask-dot" style={{ background: MASK_KINDS[lookOf(s)].dot }} />
                        {MASK_KINDS[lookOf(s)].label}
                      </button>
                      {mode && (
                        <select
                          className="mask-shape-mode"
                          aria-label={`How the ${MASK_KINDS[lookOf(s)].label.toLowerCase()} combines`}
                          title={COMBINE_MODES[mode].hint}
                          value={mode}
                          disabled={disabled}
                          onChange={(e) => tool.setShapeMode(i, e.target.value as Combine)}
                        >
                          {MODES.map((m) => (
                            <option key={m} value={m}>
                              {COMBINE_MODES[m].label}
                            </option>
                          ))}
                        </select>
                      )}
                      <button
                        className="mask-row-icon"
                        aria-label="Remove shape"
                        title="Remove shape"
                        disabled={disabled}
                        onClick={() => tool.removeShape(i)}
                      >
                        <CloseIcon size={12} />
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
            {(["add", "subtract"] as const).map((mode) => (
              <div key={mode} className={tool.addable.length > 3 ? "mask-combine many" : "mask-combine"} role="group" aria-label={`${COMBINE_MODES[mode].label} a shape`}>
                <span className="mask-combine-label">{COMBINE_MODES[mode].label}</span>
                {tool.addable.map((k) => (
                  <button
                    key={k}
                    className="ghost small"
                    title={`${COMBINE_MODES[mode].hint}: ${MASK_KINDS[k].hint.toLowerCase()}`}
                    disabled={disabled || tool.finding !== null}
                    onClick={() => tool.addShape(mode, k)}
                  >
                    {MASK_KINDS[k].add}
                  </button>
                ))}
              </div>
            ))}
          </div>
          <div className="mask-shape-controls">
            {shape?.kind === "brush" && (
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
                    disabled={disabled || shape.strokes.length === 0}
                    title="Remove everything painted with this brush"
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
            {shape?.kind === "radial" && (
              <Slider
                id="mask-feather"
                spec={feather}
                value={shape.feather}
                shown={formatSliderValue(shape.feather, feather.min, feather.step, feather.unit)}
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
        {tool.addable.map((k) => (
          <button
            key={k}
            className="chip mask-add-tile"
            title={tool.finding === k ? FINDING[k] : MASK_KINDS[k].hint}
            disabled={disabled || tool.finding !== null}
            aria-busy={tool.finding === k}
            onClick={() => tool.add(k)}
          >
            <PlusIcon size={14} />
            {tool.finding === k ? "Finding…" : MASK_KINDS[k].add}
          </button>
        ))}
      </div>
    </div>
  );
}
