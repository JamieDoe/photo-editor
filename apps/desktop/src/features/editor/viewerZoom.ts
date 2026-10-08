import { createContext, useContext } from "react";
import type { BoxPart } from "./zoom";

/** How the viewer shows the photo's box to the overlays drawn in it (ADR 0070). */
export interface ViewerZoom {
  /** The part of the box in view at 100 %; null when all of it is (Fit). */
  visible: BoxPart | null;
  /** The box's size on screen over its size at Fit (1 at Fit). */
  magnification: number;
}

export const ViewerZoomContext = createContext<ViewerZoom>({ visible: null, magnification: 1 });

export const useViewerZoom = () => useContext(ViewerZoomContext);
