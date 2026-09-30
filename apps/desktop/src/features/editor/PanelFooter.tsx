import { useCallback, useState } from "react";
import { CheckIcon, ChevronIcon, CopyIcon, PasteIcon, ResetIcon } from "../../components/icons";
import { Popover, PopoverHeader, PopoverIcon } from "../../components/Popover";
import type { Editor } from "./useEditor";

/**
 * The design's panel footer: Reset, then Copy and Paste (ADR 0048). Copy takes the
 * chosen groups of the open photo's edit (the chevron chooses them); Paste sets them
 * on the photo open now, as one undo step. The shortcuts ⇧⌘C and ⇧⌘V do the same.
 */
export function PanelFooter({
  editor,
  edited,
  copy,
  paste,
}: {
  editor: Editor;
  /** The photo differs from its original look (Reset has something to do). */
  edited: boolean;
  copy: () => void;
  paste: () => void;
}) {
  const [choosing, setChoosing] = useState<HTMLElement | null>(null);
  const close = useCallback(() => setChoosing(null), []);
  const open = editor.image !== null;
  const groups = editor.info?.settingGroups ?? [];
  const toggle = (id: string) =>
    editor.setCopyGroups(editor.copyGroups.includes(id) ? editor.copyGroups.filter((g) => g !== id) : [...editor.copyGroups, id]);
  return (
    <div className="panel-footer">
      <button
        className="footer-button"
        disabled={!open || !edited}
        title="Back to the original look (the file itself was never changed)"
        onClick={editor.resetRecipe}
      >
        <ResetIcon />
        Reset
      </button>
      <span className="footer-spacer" />
      <span className="split-button">
        <button className="footer-button" disabled={!open} title="Copy this photo’s edits (⇧⌘C)" onClick={copy}>
          <CopyIcon />
          Copy
        </button>
        <button
          className="footer-button split-more"
          disabled={!open}
          aria-label="Choose what to copy"
          aria-expanded={choosing !== null}
          title="Choose what to copy"
          onClick={(e) => {
            const anchor = e.currentTarget;
            setChoosing((c) => (c ? null : anchor));
          }}
        >
          <ChevronIcon size={12} />
        </button>
      </span>
      <button
        className="footer-button"
        disabled={!open || editor.copied === null}
        title={editor.copied ? "Paste the copied edits onto this photo (⇧⌘V)" : "Copy a photo’s edits first"}
        onClick={paste}
      >
        <PasteIcon />
        Paste
      </button>
      {choosing && (
        <Popover anchor={choosing} label="Choose what to copy" onClose={close}>
          <PopoverHeader
            visual={
              <PopoverIcon>
                <CopyIcon size={16} />
              </PopoverIcon>
            }
            title={<span className="popover-title">Copy settings</span>}
            sub="What Copy takes from a photo, and Paste sets on another"
          />
          <div className="popover-menu" role="group" aria-label="Settings to copy">
            {groups.map((g) => {
              const on = editor.copyGroups.includes(g.id);
              return (
                <button key={g.id} className="popover-item check-item" role="menuitemcheckbox" aria-checked={on} onClick={() => toggle(g.id)}>
                  <span className="check-box" aria-hidden="true">
                    {on && <CheckIcon size={11} strokeWidth={2.4} />}
                  </span>
                  {g.label}
                </button>
              );
            })}
          </div>
          <div className="popover-body">
            <div className="popover-actions">
              <button
                className="primary small"
                disabled={editor.copyGroups.length === 0}
                onClick={() => {
                  copy();
                  close();
                }}
              >
                Copy
              </button>
            </div>
          </div>
        </Popover>
      )}
    </div>
  );
}
