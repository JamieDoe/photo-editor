import { useEffect, useState } from "react";
import type { Zoom } from "./zoom";
import { ZoomModel } from "./zoomModel";

export type { ViewPoint, ZoomControl, ZoomGeometry } from "./zoomModel";

/**
 * The editor's zoom (ADR 0070), as React state: see `ZoomModel`. The model lives as
 * long as the component; `enabled` false (a tool that can't zoom) fits.
 */
export function useZoom(enabled: boolean) {
  const [zoom, setZoom] = useState<Zoom | null>(null);
  const [model] = useState(
    () =>
      new ZoomModel({
        onChange: setZoom,
        requestFrame: (cb) => requestAnimationFrame(cb),
        cancelFrame: (h) => cancelAnimationFrame(h),
        now: () => performance.now(),
      }),
  );
  useEffect(() => {
    if (!enabled) model.reset();
  }, [enabled, model]);
  useEffect(() => () => model.dispose(), [model]);
  return { zoom: enabled ? zoom : null, model };
}
