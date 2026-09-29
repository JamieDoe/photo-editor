/**
 * The on-screen size of a photo in the viewer: as large as fits the available space,
 * keeping its aspect ratio. Depends only on the photo's shape, never on how many pixels
 * the current frame has, so the camera preview, drag renders and the detail render all
 * occupy exactly the same box and only sharpen in place.
 */
export function fitSize(
  available: { width: number; height: number },
  frame: { width: number; height: number },
): { width: number; height: number } {
  if (available.width <= 0 || available.height <= 0 || frame.width <= 0 || frame.height <= 0) {
    return { width: 0, height: 0 };
  }
  const scale = Math.min(available.width / frame.width, available.height / frame.height);
  return { width: Math.floor(frame.width * scale), height: Math.floor(frame.height * scale) };
}

/** The shape the viewer's box keeps while one photo is open. */
export interface BoxShape {
  /** The open image this shape belongs to, or "opening" while only its camera preview is shown. */
  owner: number | "opening";
  width: number;
  height: number;
}

type ShownFrame =
  | { source: "embedded"; frame: { width: number; height: number } }
  | { source: "render"; imageId: number; frame: { width: number; height: number } };

/**
 * The box shape after `shown` is displayed. The first frame of each opening sets it,
 * and every later frame of that photo (sharper renders, after edits) keeps it. The
 * camera's preview and the sensor image differ in shape by a fraction of a percent,
 * too little to see when drawn into the same box, but enough to make the photo
 * visibly twitch if the box followed each frame.
 */
export function nextBoxShape(previous: BoxShape | null, shown: ShownFrame, fullSize: { width: number; height: number } | null): BoxShape {
  if (shown.source === "embedded") {
    // A camera preview starts a new opening, unless this opening already has one.
    return previous?.owner === "opening" ? previous : { owner: "opening", ...shown.frame };
  }
  if (previous?.owner === "opening") return { ...previous, owner: shown.imageId };
  if (previous?.owner === shown.imageId) return previous;
  // A new image without a camera preview: its full size gives the exact shape.
  return { owner: shown.imageId, ...(fullSize ?? shown.frame) };
}
