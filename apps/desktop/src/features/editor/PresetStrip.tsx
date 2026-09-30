import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { MoreIcon } from "../../components/icons";
import { Popover } from "../../components/Popover";
import * as ipc from "../../ipc/client";
import type { PreviewFrame } from "../../ipc/frame";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PresetDto } from "../../ipc/generated/PresetDto";
import { applyPreset, appliedPreset, hasLook, photoKey, stableStringify } from "./presets";
import type { Editor } from "./useEditor";

/** Quiet time after the photo's own settings change before previews are redrawn. */
const PREVIEW_DELAY_MS = 300;

/** The built-in presets and the photographer's own, and the changes to them. */
function usePresets(onError: (e: unknown) => void) {
  const [presets, setPresets] = useState<PresetDto[]>([]);
  useEffect(() => {
    ipc.listPresets().then(setPresets, onError);
  }, [onError]);
  const run = async <T,>(call: () => Promise<T>, then: (result: T) => void) => {
    try {
      then(await call());
      return true;
    } catch (e) {
      onError(e);
      return false;
    }
  };
  return {
    presets,
    /** Resolves to the new preset, or null if it could not be saved. */
    create: async (name: string, recipe: EditRecipe) => {
      let saved: PresetDto | null = null;
      await run(
        () => ipc.createPreset(name, recipe),
        (p) => {
          saved = p;
          setPresets((all) => [...all, p]);
        },
      );
      return saved as PresetDto | null;
    },
    rename: (id: string, name: string) =>
      run(
        () => ipc.renamePreset(id, name),
        () => setPresets((all) => all.map((p) => (p.id === id ? { ...p, name: name.trim().replace(/\s+/g, " ") } : p))),
      ),
    update: (id: string, recipe: EditRecipe) =>
      run(
        () => ipc.updatePreset(id, recipe),
        (look) => setPresets((all) => all.map((p) => (p.id === id ? { ...p, recipe: look } : p))),
      ),
    remove: (id: string) => run(() => ipc.deletePreset(id), () => setPresets((all) => all.filter((p) => p.id !== id))),
  };
}

/**
 * Small renders of the open photo with each preset. They depend only on the photo's
 * own settings (a preset replaces the rest), so moving a look slider redraws none; a
 * change to exposure, geometry or masks redraws them, one at a time, shortly after
 * it stops.
 */
function usePresetPreviews(editor: Editor, recipe: EditRecipe | null, presets: PresetDto[]) {
  const cache = useRef(new Map<string, { key: string; frame: PreviewFrame }>());
  const [, redraw] = useState(0);
  const imageId = editor.image?.id ?? null;
  const photo = recipe ? photoKey(recipe) : "";
  const list = presets.map((p) => `${p.id}=${stableStringify(p.recipe)}`).join("|");
  const { renderPresetPreview } = editor;
  useEffect(() => {
    if (!recipe || imageId === null) return;
    let live = true;
    const timer = window.setTimeout(async () => {
      for (const p of presets) {
        const key = `${imageId}:${photo}:${stableStringify(p.recipe)}`;
        if (cache.current.get(p.id)?.key === key) continue;
        try {
          const frame = await renderPresetPreview(applyPreset(recipe, p));
          if (!live) return;
          cache.current.set(p.id, { key, frame });
          redraw((n) => n + 1);
        } catch (e) {
          // Superseded by newer previews, or a failure: the tile keeps what it had.
          if (!live) return;
          if (!ipc.isCancellation(e)) console.warn("preset preview failed", e);
        }
      }
    }, PREVIEW_DELAY_MS);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
    // `photo` and `list` stand for the recipe's and presets' parts that matter.
  }, [imageId, photo, list, renderPresetPreview]);
  return (id: string) => {
    const hit = cache.current.get(id);
    return hit && imageId !== null && hit.key.startsWith(`${imageId}:`) ? hit.frame : null;
  };
}

function PresetPreview({ frame }: { frame: PreviewFrame | null }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas || !frame) return;
    canvas.width = frame.width;
    canvas.height = frame.height;
    canvas.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  }, [frame]);
  return <canvas ref={ref} className={frame ? "preset-image" : "preset-image empty"} aria-hidden="true" />;
}

/** The popover open: saving the current look, or editing a saved preset. */
type Open = { kind: "save"; anchor: HTMLElement } | { kind: "edit"; id: string; anchor: HTMLElement } | null;

/**
 * The design's preset strip (ADR 0046): the built-in looks and the photographer's own
 * as small previews of the photo; clicking one applies it (one undo step). Save…
 * keeps the current look as a preset; a saved preset's ⋯ opens its details: its
 * name, updating it to the current look, and deleting it.
 */
export function PresetStrip({ editor, recipe, disabled }: { editor: Editor; recipe: EditRecipe | null; disabled: boolean }) {
  const { presets, create, rename, update, remove } = usePresets(editor.reportError);
  const previewOf = usePresetPreviews(editor, recipe, presets);
  const [open, setOpen] = useState<Open>(null);
  const close = useCallback(() => setOpen(null), []);
  const editing = open?.kind === "edit" ? (presets.find((p) => p.id === open.id) ?? null) : null;
  // The preset last chosen on each photo this session (by path), so that of several
  // presets sharing a look, the one chosen is the one shown as applied.
  const [chosen, setChosen] = useState<Record<string, string>>({});
  const path = editor.image?.path ?? "";
  const choose = (id: string) => setChosen((c) => ({ ...c, [path]: id }));
  const applied = recipe ? appliedPreset(recipe, presets, chosen[path]) : null;

  return (
    <div className="presets">
      <div className="presets-header">
        <span className="presets-title">Presets</span>
        <button
          className="ghost small"
          disabled={disabled || !recipe}
          title="Keep this photo’s look as a preset"
          onClick={(e) => {
            const anchor = e.currentTarget;
            setOpen((o) => (o?.kind === "save" ? null : { kind: "save", anchor }));
          }}
        >
          Save…
        </button>
      </div>
      <div className="preset-strip" role="list">
        {presets.map((p) => {
          const on = p.id === applied;
          return (
            <div key={p.id} className={open?.kind === "edit" && open.id === p.id ? "preset-tile editing" : "preset-tile"} role="listitem">
              <button
                className="preset-apply"
                aria-pressed={on}
                disabled={disabled || !recipe}
                title={`Apply ${p.name}`}
                onClick={() => {
                  if (!recipe) return;
                  editor.setRecipe(applyPreset(recipe, p), p.name);
                  choose(p.id);
                }}
              >
                <PresetPreview frame={previewOf(p.id)} />
                <span className="preset-name">{p.name}</span>
              </button>
              {!p.builtIn && (
                <button
                  className="preset-more"
                  aria-label={`Edit ${p.name}`}
                  aria-expanded={open?.kind === "edit" && open.id === p.id}
                  title="Edit preset"
                  disabled={disabled}
                  onClick={(e) => {
                    const anchor = e.currentTarget;
                    setOpen((o) => (o?.kind === "edit" && o.id === p.id ? null : { kind: "edit", id: p.id, anchor }));
                  }}
                >
                  <MoreIcon size={12} />
                </button>
              )}
            </div>
          );
        })}
      </div>
      {open?.kind === "save" && recipe && (
        <Popover anchor={open.anchor} label="Save preset" onClose={close}>
          <NameForm
            title="Save this look as a preset"
            initial=""
            action="Save"
            onSubmit={async (name) => {
              const saved = await create(name, recipe);
              if (saved) {
                choose(saved.id);
                close();
              }
            }}
            onCancel={close}
          />
        </Popover>
      )}
      {open?.kind === "edit" && editing && (
        <Popover anchor={open.anchor} label={`Edit ${editing.name}`} onClose={close}>
          <PresetDetails
            key={editing.id}
            preset={editing}
            canUpdate={recipe !== null && !hasLook(recipe, editing)}
            onRename={(name) => rename(editing.id, name)}
            onUpdate={async () => {
              if (recipe && (await update(editing.id, recipe))) {
                choose(editing.id);
                close();
              }
            }}
            onDelete={async () => (await remove(editing.id)) && close()}
          />
        </Popover>
      )}
    </div>
  );
}

/** A name field with its button: Enter or the button submits, Escape closes. */
function NameForm({
  title,
  initial,
  action,
  onSubmit,
  onCancel,
}: {
  title: string;
  initial: string;
  action: string;
  onSubmit: (name: string) => void;
  onCancel?: () => void;
}) {
  const [name, setName] = useState(initial);
  const changed = name.trim() !== "" && name.trim() !== initial.trim();
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (changed) onSubmit(name);
  };
  return (
    <form className="popover-section" onSubmit={submit}>
      <label className="popover-label" htmlFor="preset-name">
        {title}
      </label>
      <div className="popover-row">
        <input
          id="preset-name"
          className="text-input"
          autoFocus
          maxLength={60}
          placeholder="Preset name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button className="primary small" type="submit" disabled={!changed}>
          {action}
        </button>
      </div>
      {onCancel && (
        <div className="popover-actions">
          <button className="ghost small" type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      )}
    </form>
  );
}

/** A saved preset's details: rename it, update it to the photo's look, delete it. */
function PresetDetails({
  preset,
  canUpdate,
  onRename,
  onUpdate,
  onDelete,
}: {
  preset: PresetDto;
  canUpdate: boolean;
  onRename: (name: string) => void;
  onUpdate: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  return (
    <>
      <NameForm title="Name" initial={preset.name} action="Rename" onSubmit={onRename} />
      <div className="popover-section">
        <button
          className="popover-item"
          disabled={!canUpdate}
          title={canUpdate ? undefined : "The photo already has this look"}
          onClick={onUpdate}
        >
          <span>Update to this photo’s look</span>
          <span className="popover-item-sub">Replaces the preset’s settings</span>
        </button>
        {confirming ? (
          <div className="popover-confirm">
            <span>Delete “{preset.name}”? Photos keep their edits.</span>
            <div className="popover-actions">
              <button className="ghost small" onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button className="danger small" autoFocus onClick={onDelete}>
                Delete
              </button>
            </div>
          </div>
        ) : (
          <button className="popover-item danger" onClick={() => setConfirming(true)}>
            <span>Delete preset…</span>
          </button>
        )}
      </div>
    </>
  );
}
