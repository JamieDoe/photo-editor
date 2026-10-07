import type { MarkChangeDto } from "../ipc/generated/MarkChangeDto";
import type { MarksDto } from "../ipc/generated/MarksDto";
import { flagClick, starClick } from "../features/library/marks";
import { LabelPicker } from "./ColourLabels";
import { PickIcon, RatingStar, RejectIcon } from "./icons";

interface Props {
  marks: MarksDto;
  onChange: (change: MarkChangeDto) => void;
}

/** Five stars and pick/reject, as in the design's Edit toolbar, and the colour labels
 *  (ADR 0064). */
export function MarkControls({ marks, onChange }: Props) {
  return (
    <div className="mark-controls">
      <div role="radiogroup" aria-label="Rating" className="stars">
        {[1, 2, 3, 4, 5].map((n) => (
          <button
            key={n}
            role="radio"
            aria-checked={marks.rating === n}
            aria-label={`${n} star${n > 1 ? "s" : ""}`}
            title={marks.rating === n ? "Clear rating (0)" : `Rate ${n} (${n})`}
            onClick={() => onChange(starClick(marks.rating, n))}
          >
            <RatingStar filled={n <= marks.rating} />
          </button>
        ))}
      </div>
      <span className="toolbar-divider" />
      <button
        className="flag-button pick"
        aria-pressed={marks.flag === "pick"}
        aria-label="Flag as pick"
        title="Pick (P)"
        onClick={() => onChange(flagClick(marks.flag, "pick"))}
      >
        <PickIcon filled={marks.flag === "pick"} />
      </button>
      <button
        className="flag-button reject"
        aria-pressed={marks.flag === "reject"}
        aria-label="Reject"
        title="Reject (X)"
        onClick={() => onChange(flagClick(marks.flag, "reject"))}
      >
        <RejectIcon />
      </button>
      <span className="toolbar-divider" />
      <LabelPicker value={marks.label} onChange={onChange} />
    </div>
  );
}
