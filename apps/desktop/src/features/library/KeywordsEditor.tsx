import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { CloseIcon, PlusIcon, TagIcon } from "../../components/icons";
import { Popover, PopoverHeader, PopoverIcon } from "../../components/Popover";
import * as ipc from "../../ipc/client";
import type { KeywordUseDto } from "../../ipc/generated/KeywordUseDto";
import { photosText } from "./Albums";
import { MAX_KEYWORD_CHARS, completeFragment, keywordSuggestions, splitKeywords } from "./keywords";

/**
 * The keywords of some photos (ADR 0088): each one they have, with how many of them
 * when not all, to take off (×) or give to all (+); a field to add more, several at
 * once with commas, suggesting the library's own as they are typed.
 */
export function KeywordsEditor({
  paths,
  anchor,
  onClose,
  onChanged,
  onError,
}: {
  paths: string[];
  anchor: HTMLElement;
  onClose: () => void;
  /** Keywords changed: smart collections and searches may hold other photos now. */
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  const [on, setOn] = useState<KeywordUseDto[] | null>(null);
  const [library, setLibrary] = useState<KeywordUseDto[]>([]);
  const [text, setText] = useState("");
  const [active, setActive] = useState(-1);
  const count = paths.length;
  // Focused once the popover is placed (it is hidden for its first frame, and a hidden
  // field can't take focus), so typing goes straight in.
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const frame = requestAnimationFrame(() => field.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    let live = true;
    ipc.photoKeywords(paths).then((k) => live && setOn(k), onError);
    ipc.listKeywords().then((k) => live && setLibrary(k), () => undefined);
    return () => {
      live = false;
    };
    // The photos are fixed while the popover is open.
  }, []);

  const change = async (run: () => Promise<KeywordUseDto[]>) => {
    try {
      setOn(await run());
      onChanged();
      ipc.listKeywords().then(setLibrary, () => undefined);
    } catch (e) {
      onError(e);
    }
  };
  const add = (names: string[]) => names.length > 0 && void change(() => ipc.addKeywords(paths, names));
  const remove = (name: string) => void change(() => ipc.removeKeywords(paths, [name]));

  const suggestions = keywordSuggestions(library, text, on ?? [], count);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    const chosen = suggestions[active];
    add(splitKeywords(chosen ? completeFragment(text, chosen) : text));
    setText("");
    setActive(-1);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape" && text) {
      // Clears the field first; a second Escape closes the popover.
      e.stopPropagation();
      setText("");
      setActive(-1);
      return;
    }
    if (suggestions.length === 0) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      // Through the suggestions and back to the field (-1).
      const n = suggestions.length;
      const step = e.key === "ArrowDown" ? 1 : -1;
      setActive((a) => (a + step < -1 ? n - 1 : a + step >= n ? -1 : a + step));
    } else if (e.key === "Tab") {
      e.preventDefault();
      setText(completeFragment(text, suggestions[Math.max(0, active)]!) + ", ");
      setActive(-1);
    }
  };

  const sub = on === null ? "Loading…" : `${photosText(count)}${on.length > 0 ? ` · ${on.length} keyword${on.length === 1 ? "" : "s"}` : ""}`;
  return (
    <Popover anchor={anchor} label="Keywords" onClose={onClose}>
      <PopoverHeader
        visual={
          <PopoverIcon>
            <TagIcon size={18} />
          </PopoverIcon>
        }
        title={<span className="popover-title">Keywords</span>}
        sub={sub}
      />
      <div className="popover-body keywords-body">
        {on !== null && on.length === 0 && (
          <p className="popover-text small">No keywords yet. Add words to find {count === 1 ? "it" : "them"} by later.</p>
        )}
        {on !== null && on.length > 0 && (
          <ul className="keyword-chips" aria-label="Keywords of these photos">
            {on.map((k) => {
              const partial = k.photos < count;
              return (
                <li key={k.name} className={partial ? "keyword-chip partial" : "keyword-chip"}>
                  {partial && (
                    <button
                      className="keyword-chip-button"
                      aria-label={`Add ${k.name} to all ${count} photos`}
                      title={`On ${k.photos} of ${count}: add to all`}
                      onClick={() => add([k.name])}
                    >
                      <PlusIcon size={10} />
                    </button>
                  )}
                  <span className="keyword-name">{k.name}</span>
                  {partial && <span className="keyword-count">{`${k.photos}/${count}`}</span>}
                  <button
                    className="keyword-chip-button"
                    aria-label={`Remove ${k.name}`}
                    title={count === 1 ? "Remove" : "Remove from all"}
                    onClick={() => remove(k.name)}
                  >
                    <CloseIcon size={9} />
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        <form className="keyword-form" onSubmit={submit}>
          <input
            ref={field}
            className="text-input"
            maxLength={MAX_KEYWORD_CHARS * 8}
            placeholder="Add keywords"
            aria-label="Add keywords"
            role="combobox"
            aria-expanded={suggestions.length > 0}
            aria-controls="keyword-suggestions"
            aria-activedescendant={active >= 0 ? `keyword-suggestion-${active}` : undefined}
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setActive(-1);
            }}
            onKeyDown={onKey}
          />
          {suggestions.length > 0 && (
            <ul className="keyword-suggestions" id="keyword-suggestions" role="listbox">
              {suggestions.map((name, i) => (
                <li
                  key={name}
                  id={`keyword-suggestion-${i}`}
                  role="option"
                  aria-selected={i === active}
                  onPointerDown={(e) => {
                    // Keeps the field's focus.
                    e.preventDefault();
                    add(splitKeywords(completeFragment(text, name)));
                    setText("");
                    setActive(-1);
                  }}
                >
                  {name}
                </li>
              ))}
            </ul>
          )}
        </form>
        <p className="popover-text small">Separate several with commas. Search and smart collections find photos by them.</p>
      </div>
    </Popover>
  );
}
