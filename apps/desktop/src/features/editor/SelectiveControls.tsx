import { CloseIcon, EyeIcon, EyeOffIcon, PlusIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { MaskTool } from "./MaskTool";
import { ADDABLE_KINDS, MASK_KINDS, maskName } from "./masks";
import { Slider } from "./Slider";
import { formatSliderValue } from "./sliderTrack";
import { WHITE_BALANCE_TRACKS } from "./whiteBalance";

const LOCAL_KEYS = ["exposure", "warmth", "clarity"] as const;
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
            Adjust just part of the photo: a linear gradient to darken a sky, a radial one to lift a face or darken the edges.
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
