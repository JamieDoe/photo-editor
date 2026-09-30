import { CloseIcon, PlusIcon } from "../../components/icons";
import type { AdjustmentSpec } from "../../ipc/generated/AdjustmentSpec";
import type { LocalAdjustments } from "../../ipc/generated/LocalAdjustments";
import type { MaskTool } from "./MaskTool";
import { MASK_KINDS, maskName } from "./masks";
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
export function SelectiveControls({ tool, specs, disabled }: { tool: MaskTool; specs: AdjustmentSpec[]; disabled: boolean }) {
  const active = tool.active;
  return (
    <div className="selective">
      <div className="mask-list">
        {tool.masks.map((m) => {
          const on = active?.id === m.id;
          return (
            <div key={m.id} className={on ? "mask-row active" : "mask-row"}>
              <button className="mask-row-pick" aria-pressed={on} disabled={disabled} onClick={() => tool.pick(m.id)}>
                <span className="mask-swatch">
                  <span className="mask-dot" style={{ background: MASK_KINDS[m.shape.kind].dot }} />
                </span>
                <span className="mask-row-text">
                  <span className="mask-row-name">{maskName(tool.masks, m)}</span>
                  <span className="mask-row-kind">{MASK_KINDS[m.shape.kind].label}</span>
                </span>
              </button>
              <button className="mask-remove" aria-label="Delete mask" title="Delete mask" disabled={disabled} onClick={() => tool.remove(m.id)}>
                <CloseIcon size={12} />
              </button>
            </div>
          );
        })}
        {tool.masks.length === 0 && (
          <div className="mask-empty">Adjust just part of the photo. Start with a linear gradient to darken a sky or lighten a foreground.</div>
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
        </div>
      )}
      <div className="mask-add-grid">
        <button className="chip mask-add-tile" title="Graduated filter" disabled={disabled} onClick={() => tool.add("linear")}>
          <PlusIcon size={14} />
          Linear
        </button>
      </div>
    </div>
  );
}
