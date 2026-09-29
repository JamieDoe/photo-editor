/** Grid and size from the design (a 288 x 120 viewBox, quarters marked). */
const W = 288;
const H = 120;

/**
 * The Light section's tone curve, as in the design: what the Light controls do to
 * each tone, against the default rendering (the dashed diagonal). Display only; the
 * points come from the renderer (ADR 0029).
 */
export function ToneCurve({ points }: { points: number[] | null }) {
  const path =
    points && points.length > 1
      ? points
          .map((y, i) => {
            const x = (i / (points.length - 1)) * W;
            return `${i ? "L" : "M"}${x.toFixed(1)} ${(H - Math.min(1, Math.max(0, y)) * H).toFixed(1)}`;
          })
          .join(" ")
      : null;
  return (
    <>
      <div className="tone-curve-title">Tone curve</div>
      <div className="tone-curve">
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label="Tone curve">
          {[72, 144, 216].map((x) => (
            <line key={`x${x}`} className="grid" x1={x} y1={0} x2={x} y2={H} />
          ))}
          {[30, 60, 90].map((y) => (
            <line key={`y${y}`} className="grid" x1={0} y1={y} x2={W} y2={y} />
          ))}
          <line className="identity" x1={0} y1={H} x2={W} y2={0} />
          {path && <path className="curve" d={path} />}
        </svg>
      </div>
    </>
  );
}
