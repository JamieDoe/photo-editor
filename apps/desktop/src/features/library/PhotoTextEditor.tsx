import { useEffect, useRef, useState } from "react";
import { CaptionIcon } from "../../components/icons";
import { Popover, PopoverHeader, PopoverIcon } from "../../components/Popover";
import * as ipc from "../../ipc/client";
import type { PhotoTextDto } from "../../ipc/generated/PhotoTextDto";
import type { TextFieldDto } from "../../ipc/generated/TextFieldDto";
import { photosText } from "./Albums";

/** How long typing rests before a field is saved. */
const SAVE_DELAY_MS = 500;

const FIELDS: ReadonlyArray<{ field: TextFieldDto; label: string; max: number }> = [
  { field: "title", label: "Title", max: 200 },
  { field: "caption", label: "Caption", max: 2000 },
  { field: "creator", label: "Creator", max: 200 },
  { field: "copyright", label: "Copyright", max: 200 },
];

/**
 * Some photos' words (ADR 0089): a title and caption, and their own creator and
 * copyright in place of the usual ones from Settings. A field the photos differ in says
 * so and is left alone unless typed into. Each field is saved as typing rests, and when
 * the popover closes.
 */
export function PhotoTextEditor({
  paths,
  anchor,
  onClose,
  onChanged,
  onError,
}: {
  paths: string[];
  anchor: HTMLElement;
  onClose: () => void;
  /** Words changed: searches may find other photos now. */
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const [loaded, setLoaded] = useState<PhotoTextDto | null>(null);
  // What is typed, by field; a field not in it shows what the photos have.
  const [typed, setTyped] = useState<Partial<Record<TextFieldDto, string>>>({});
  const pending = useRef(new Map<TextFieldDto, number>());
  const typedRef = useRef(typed);
  typedRef.current = typed;
  const first = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let live = true;
    ipc.photoText(paths).then((t) => live && setLoaded(t), onError);
    return () => {
      live = false;
    };
    // The photos are fixed while the popover is open.
  }, []);

  const save = (field: TextFieldDto) => {
    pending.current.delete(field);
    const text = typedRef.current[field];
    if (text === undefined) return;
    ipc.setPhotoText(paths, field, text).then(onChanged, onError);
  };
  // Saves what is still waiting when the popover closes.
  useEffect(
    () => () => {
      for (const [field, timer] of pending.current) {
        window.clearTimeout(timer);
        save(field);
      }
    },
    [],
  );
  // Focused once the popover is placed (it is hidden for its first frame).
  useEffect(() => {
    const frame = requestAnimationFrame(() => first.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [loaded !== null]);

  const type = (field: TextFieldDto, text: string) => {
    setTyped((t) => ({ ...t, [field]: text }));
    typedRef.current = { ...typedRef.current, [field]: text };
    window.clearTimeout(pending.current.get(field));
    pending.current.set(
      field,
      window.setTimeout(() => save(field), SAVE_DELAY_MS),
    );
  };

  const placeholder = (field: TextFieldDto): string => {
    if (!loaded) return "";
    if (loaded[field].mixed) return "Mixed: type to set for all";
    if (field === "creator") return loaded.defaultCreator ? `From Settings: ${loaded.defaultCreator}` : "None (set yours in Settings)";
    if (field === "copyright") return loaded.defaultCopyright ? `From Settings: ${loaded.defaultCopyright}` : "None (set yours in Settings)";
    return field === "title" ? "A short title" : "What, where, who";
  };

  return (
    <Popover anchor={anchor} label="Title and caption" onClose={onClose}>
      <PopoverHeader
        visual={
          <PopoverIcon>
            <CaptionIcon size={18} />
          </PopoverIcon>
        }
        title={<span className="popover-title">Title and caption</span>}
        sub={loaded ? photosText(paths.length) : "Loading…"}
      />
      <div className="popover-body photo-text">
        {FIELDS.map(({ field, label, max }) => {
          const value = typed[field] ?? loaded?.[field].value ?? "";
          const common = {
            className: "text-input",
            maxLength: max,
            disabled: loaded === null,
            placeholder: placeholder(field),
            value,
            "aria-label": label,
          };
          return (
            <label key={field} className={field === "creator" ? "photo-text-row credits-start" : "photo-text-row"}>
              <span>{label}</span>
              {field === "caption" ? (
                <textarea {...common} rows={3} onChange={(e) => type(field, e.target.value)} />
              ) : (
                <input
                  {...common}
                  ref={field === "title" ? first : undefined}
                  onChange={(e) => type(field, e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      onClose();
                    }
                  }}
                />
              )}
            </label>
          );
        })}
        <p className="popover-text small">
          Exports carry these when Keep metadata is on; search finds titles and captions. Leave creator and copyright empty
          to use yours from Settings.
        </p>
      </div>
    </Popover>
  );
}
