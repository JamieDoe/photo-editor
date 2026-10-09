import { useEffect, useRef, useState } from "react";
import * as ipc from "../../ipc/client";
import type { ChromaticAberration } from "../../ipc/generated/ChromaticAberration";
import type { EditRecipe } from "../../ipc/generated/EditRecipe";

const SUBTITLE = "Cleans colour fringing on edges";

/**
 * The design's "Lens correction" switch (ADR 0075): the lens's own corrections,
 * distortion and vignetting, from the profile the camera recorded in the file. On
 * unless turned off; without a profile there is nothing to apply.
 */
export function LensCorrectionToggle({
  recipe,
  lens,
  onChange,
  disabled,
}: {
  recipe: EditRecipe;
  /** The lens whose profile the photo's file has; null without one. */
  lens: string | null;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
}) {
  const on = lens !== null && recipe.profileCorrections !== false;
  return (
    <button
      className="lens-toggle"
      aria-pressed={on}
      disabled={disabled || lens === null}
      onClick={() => onChange({ ...recipe, profileCorrections: on ? false : undefined })}
    >
      <span className="lens-toggle-text">
        <span className="lens-toggle-label">Lens correction</span>
        <span className="lens-toggle-sub">{lens === null ? "No lens profile in this photo’s file" : `${lens} · auto`}</span>
      </span>
      <span className="switch" aria-hidden="true">
        <span className="switch-knob" />
      </span>
    </button>
  );
}

/** How far red or blue moves at the corners of a `w` x `h` photo, in pixels. */
export function largestShift(ca: ChromaticAberration, w: number, h: number): number {
  const r = 0.5 * Math.hypot(w, h);
  return Math.max(Math.abs(ca.red[0] + ca.red[1]), Math.abs(ca.blue[0] + ca.blue[1])) * r;
}

/**
 * The design's "Remove chromatic aberration" switch (ADR 0035). Turning it on measures
 * the photo's colour fringing in the renderer and keeps the correction in the recipe,
 * so previews and the export use the same numbers.
 */
export function ChromaticAberrationToggle({
  recipe,
  imageId,
  size,
  onChange,
  disabled,
}: {
  recipe: EditRecipe;
  imageId: number | null;
  size: { width: number; height: number } | null;
  onChange: (r: EditRecipe) => void;
  disabled: boolean;
}) {
  const [measuring, setMeasuring] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  // The measurement takes a moment: apply it to the latest edit of the same photo.
  const latest = useRef({ recipe, imageId });
  latest.current = { recipe, imageId };
  useEffect(() => {
    if (status === null) return;
    const t = window.setTimeout(() => setStatus(null), 3000);
    return () => window.clearTimeout(t);
  }, [status]);
  useEffect(() => setStatus(null), [imageId]);

  const on = recipe.chromaticAberration !== undefined;
  const toggle = () => {
    if (on) {
      onChange({ ...recipe, chromaticAberration: undefined });
      return;
    }
    if (imageId === null) return;
    const measured = imageId;
    setMeasuring(true);
    ipc
      .measureChromaticAberration(measured)
      .then((ca) => {
        if (latest.current.imageId !== measured) return;
        if (ca === null) {
          setStatus("Not enough clear edges to measure");
          return;
        }
        onChange({ ...latest.current.recipe, chromaticAberration: ca });
        const small = size !== null && largestShift(ca, size.width, size.height) < 0.25;
        setStatus(small ? "Hardly any fringing found" : "Colour fringing removed");
      })
      .catch(() => setStatus("Couldn’t measure the fringing"))
      .finally(() => setMeasuring(false));
  };

  return (
    <button
      className="lens-toggle"
      aria-pressed={on}
      disabled={disabled || measuring}
      onClick={toggle}
    >
      <span className="lens-toggle-text">
        <span className="lens-toggle-label">Remove chromatic aberration</span>
        <span className="lens-toggle-sub" role="status">
          {measuring ? "Measuring…" : (status ?? SUBTITLE)}
        </span>
      </span>
      <span className="switch" aria-hidden="true">
        <span className="switch-knob" />
      </span>
    </button>
  );
}
