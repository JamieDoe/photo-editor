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
  /** The open image this shape belongs to. */
  owner: number;
  width: number;
  height: number;
}

/**
 * The box shape after a render of `imageId` is shown. Each photo gets one box per
 * framing, from its full-resolution output size (exact shape), kept for all its
 * renders: preview levels round their sizes, so fitting each frame by its own shape
 * would move the photo by a pixel or two. A new crop is a new shape.
 */
export function nextBoxShape(
  previous: BoxShape | null,
  imageId: number,
  frame: { width: number; height: number },
  fullSize: { width: number; height: number } | null,
): BoxShape {
  const size = fullSize ?? frame;
  if (previous?.owner === imageId && previous.width === size.width && previous.height === size.height) {
    return previous;
  }
  return { owner: imageId, width: size.width, height: size.height };
}
