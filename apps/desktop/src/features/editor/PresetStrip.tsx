import { useEffect, useRef, useState, type FormEvent } from "react";
import { CloseIcon, MoreIcon } from "../../components/icons";
import * as ipc from "../../ipc/client";
import type { PreviewFrame } from "../../ipc/frame";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";
import type { PresetDto } from "../../ipc/generated/PresetDto";
import { applyPreset, hasLook, photoKey, stableStringify } from "./presets";
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
    create: (name: string, recipe: EditRecipe) =>
      run(() => ipc.createPreset(name, recipe), (p) => setPresets((all) => [...all, p])),
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

/** What the panel under the strip is doing: naming a new preset, or managing one. */
type Panel = { kind: "save" } | { kind: "manage"; id: string; step: "actions" | "rename" | "delete" } | null;

/**
 * The design's preset strip (ADR 0046): the built-in looks and the photographer's own
 * as small previews of the photo; clicking one applies it (one undo step). Save keeps
 * the current look as a preset; a saved preset's ⋯ renames it, updates it to the
 * current look or deletes it.
 */
export function PresetStrip({ editor, recipe, disabled }: { editor: Editor; recipe: EditRecipe | null; disabled: boolean }) {
  const { presets, create, rename, update, remove } = usePresets(editor.reportError);
  const previewOf = usePresetPreviews(editor, recipe, presets);
  const [panel, setPanel] = useState<Panel>(null);
  const [name, setName] = useState("");
  const managed = panel?.kind === "manage" ? (presets.find((p) => p.id === panel.id) ?? null) : null;

  const openSave = () => {
    setName("");
    setPanel({ kind: "save" });
  };
  const submitName = async (e: FormEvent) => {
    e.preventDefault();
    if (!recipe || !name.trim()) return;
    const ok = panel?.kind === "save" ? await create(name, recipe) : managed ? await rename(managed.id, name) : false;
    if (ok) setPanel(null);
  };

  return (
    <div className="presets">
      <div className="presets-header">
        <span className="presets-title">Presets</span>
        <button className="ghost small" disabled={disabled || !recipe} title="Keep this photo’s look as a preset" onClick={openSave}>
          Save…
        </button>
      </div>
      <div className="preset-strip" role="list">
        {presets.map((p) => {
          const on = recipe !== null && hasLook(recipe, p);
          return (
            <div key={p.id} className="preset-tile" role="listitem">
              <button
                className="preset-apply"
                aria-pressed={on}
                disabled={disabled || !recipe}
                title={`Apply ${p.name}`}
                onClick={() => recipe && editor.setRecipe(applyPreset(recipe, p), p.name)}
              >
                <PresetPreview frame={previewOf(p.id)} />
                <span className="preset-name">{p.name}</span>
              </button>
              {!p.builtIn && (
                <button
                  className="preset-more"
                  aria-label={`Change ${p.name}`}
                  title="Rename, update or delete"
                  disabled={disabled}
                  onClick={() => setPanel({ kind: "manage", id: p.id, step: "actions" })}
                >
                  <MoreIcon size={12} />
                </button>
              )}
            </div>
          );
        })}
      </div>
      {panel?.kind === "save" || (managed && panel?.kind === "manage" && panel.step === "rename") ? (
        <form className="preset-panel" onSubmit={submitName}>
          <input
            className="text-input"
            autoFocus
            maxLength={60}
            placeholder="Preset name"
            aria-label="Preset name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setPanel(null)}
          />
          <button className="primary small" type="submit" disabled={!name.trim()}>
            {panel?.kind === "save" ? "Save" : "Rename"}
          </button>
          <button className="ghost small" type="button" onClick={() => setPanel(null)}>
            Cancel
          </button>
        </form>
      ) : managed && panel?.kind === "manage" && panel.step === "delete" ? (
        <div className="preset-panel">
          <span className="preset-panel-text">Delete “{managed.name}”? Photos keep their edits.</span>
          <button className="danger small" onClick={async () => (await remove(managed.id)) && setPanel(null)}>
            Delete
          </button>
          <button className="ghost small" onClick={() => setPanel(null)}>
            Cancel
          </button>
        </div>
      ) : managed ? (
        <div className="preset-panel">
          <span className="preset-panel-text strong">{managed.name}</span>
          <button
            className="ghost small"
            onClick={() => {
              setName(managed.name);
              setPanel({ kind: "manage", id: managed.id, step: "rename" });
            }}
          >
            Rename
          </button>
          <button
            className="ghost small"
            disabled={!recipe}
            title="Replace this preset’s look with this photo’s"
            onClick={async () => recipe && (await update(managed.id, recipe)) && setPanel(null)}
          >
            Update
          </button>
          <button className="ghost small" onClick={() => setPanel({ kind: "manage", id: managed.id, step: "delete" })}>
            Delete
          </button>
          <button className="mask-row-icon" aria-label="Close" title="Close" onClick={() => setPanel(null)}>
            <CloseIcon size={12} />
          </button>
        </div>
      ) : null}
    </div>
  );
}
