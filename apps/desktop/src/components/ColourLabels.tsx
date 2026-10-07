import type { ColourLabelDto } from "../ipc/generated/ColourLabelDto";
import type { MarkChangeDto } from "../ipc/generated/MarkChangeDto";
import { LABELS, labelClick, type Label } from "../features/library/marks";

const NAMES: Record<Label, string> = Object.fromEntries(LABELS.map((l) => [l.id, l.name])) as Record<Label, string>;

/** A photo's colour label as a small dot (ADR 0064); nothing when it has none. */
export function LabelDot({ label }: { label: ColourLabelDto }) {
  if (label === "none") return null;
  return <span className="label-dot" data-label={label} role="img" aria-label={`${NAMES[label]} label`} title={`${NAMES[label]} label`} />;
}

/** The five labels to choose from; choosing the current one removes it. */
export function LabelPicker({ value, onChange }: { value: ColourLabelDto; onChange: (change: MarkChangeDto) => void }) {
  return (
    <div role="radiogroup" aria-label="Colour label" className="label-picker">
      {LABELS.map((l) => (
        <button
          key={l.id}
          role="radio"
          aria-checked={value === l.id}
          aria-label={l.name}
          title={value === l.id ? `Remove ${l.name.toLowerCase()} label${l.key ? ` (${l.key})` : ""}` : `${l.name} label${l.key ? ` (${l.key})` : ""}`}
          onClick={() => onChange(labelClick(value, l.id))}
        >
          <span className="label-swatch" data-label={l.id} />
        </button>
      ))}
    </div>
  );
}

/** Shows only photos with one label (pressed again to show all). */
export function LabelFilter({ value, onChange }: { value: Label | null; onChange: (label: Label | null) => void }) {
  return (
    <div className="label-filter" role="group" aria-label="Show one colour label">
      {LABELS.map((l) => (
        <button
          key={l.id}
          aria-pressed={value === l.id}
          aria-label={`Only ${l.name.toLowerCase()}`}
          title={value === l.id ? "Show every label" : `Only photos labelled ${l.name.toLowerCase()}`}
          onClick={() => onChange(value === l.id ? null : l.id)}
        >
          <span className="label-swatch" data-label={l.id} />
        </button>
      ))}
    </div>
  );
}
