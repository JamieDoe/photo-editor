# ADR 0058: Finding sensor dust

- Status: Accepted
- Date: 2026-10-01

## Context

Dust on a camera's sensor leaves the same soft, dark specks in every photo, most
visible in skies and other plain areas. Healing them one by one is tedious.

The design's Retouch section has a banner, "2 sensor dust spots found in the sky ·
Fix all", then "2 spots removed · Undo". On the photo, dashed amber rings mark the
spots found. Heal spots (ADR 0054) can already remove the dust; what was missing was
finding it.

## Decision

1. **Detection** (`renderer::dust`) runs on a preview level of about 2000 px, in log
   luminance, because dust darkens what is behind it by a similar fraction.
   - Each pixel is compared with its surroundings. Lightly smoothed against noise, it
     is set against a background blurred at a fiftieth of the long edge, using three
     box blurs, close to a Gaussian.
   - Patches at least about 2.5% darker than their background are gathered into blobs.
   - **A blob counts as dust only when:**
     - **size:** its radius is 0.12–2% of the long edge, soft edge included;
     - **shape:** it is round, filling at least half its box, which is at most twice as
       long as it is wide;
     - **mirror symmetry:** it is darkened about as much on each side of its centre;
     - **depth:** it is darker, but by less than about 55%, so black objects are not
       dust;
     - **neutral colour:** every channel darkens by about the same fraction, within 20%
       of green's plus a little for noise. Dust has no colour of its own; a grey cloud
       on blue sky darkens blue more than red;
     - **smooth surroundings:** the ring from 1.5 to 2.5 radii varies by less than 0.02
       in log luminance, and the blob is at least three times deeper than that.
       Texture hides dust and imitates it.
   - At most 40 are returned, scored by depth over the surroundings' unevenness.
2. **Each find is a heal spot.** Its centre and radius come from the blob, and its
   source is found as for a clicked spot (`find_source`), clear of the existing spots
   and of the other finds. Dust that a spot already covers is left out.
3. **Engine and commands:** `Engine::find_dust` and the `find_dust` command run on the
   interactive lane and supersede each other.
4. **UI**, as in the design:
   - Opening Retouch looks for dust once per photo.
   - The spots found show as dashed amber rings, and the banner reads "N sensor dust
     spots found · Fix all".
   - **Fix all** adds them all in one edit (one undo step), then shows "N spots
     removed · Undo". Undo takes out exactly those spots.
   - Clicking a ring heals just that spot, at the size found.
   - Spots made by hand now show as a quiet white ring, leaving dashed amber to mean
     "found".
5. **Speed:** the blurs run in parallel. Rows are summed along each row; columns are
   kept as running sums of whole rows, read in order. On the six real camera files,
   detection took 31–66 ms (it was 110–255 ms before the blurs were parallel).
6. **Tests:**
   - **Renderer, synthetic sky:**
     - four dust spots of different sizes and strengths are found, with sizes that
       cover their soft edges and about the darkening made;
     - a coloured blob, a black object, a thin wire and a small grey cloud are not;
     - nothing is found in a plain sky or in busy texture;
     - dust that a spot already covers is not offered again;
     - after healing, nothing is left to find;
     - the parallel blur matches a direct box blur.
   - **Frontend:** what is still to fix, and Undo of a Fix all.
   - **Real photos (`bench --dust`, six cameras):**
     - nothing on five;
     - one spot on the Ricoh GR III: soft, about 9% darker, neutral, in fairly smooth
       sky;
     - two earlier finds on that photo were small grey clouds. The neutrality, smooth
       surroundings and symmetry checks were tightened until they went.
   - **Release self-test:** on the Nikon Z 6, nothing found in 53 ms. On the Ricoh GR
     III, one found in 70 ms, healed from a real source, with nothing left after.

## Deviations from the design

Recorded in ADR 0016:

- The banner does not say where the dust is ("in the sky"): the detector does not
  classify scenes.
- Clicking a ring to fix one spot is an addition.

## Consequences

- **Finding dust across many photos** at once, which would be the same spots in each,
  could later use the batch selection (ADR 0049).
- **Confidence:** the thresholds are tuned on synthetic dust and six real photos. More
  real dust (it shows best at small apertures) would tell whether they are right.
