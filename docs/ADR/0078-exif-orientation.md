# ADR 0078: Photos shown upright by their EXIF orientation

- Status: Accepted
- Date: 2026-10-09

## Context

Phones, and most cameras for portrait shots, store the image as the sensor read it and
record how to show it upright in EXIF Orientation (1 to 8: four turns, each optionally
mirrored).
- **Raw files:** LibRaw turns them upright.
- **JPEG, PNG and TIFF:** they were decoded as stored, so a portrait phone photo
  opened, edited and exported sideways. The library already read the orientation,
  but only for the photos' sizes.

## Decision

1. **Turn rendered files upright at decode.**
   - **Where:** `raw::rendered::upright` for the edited image; `orient_to_rgba` for
     thumbnails.
   - **How:** EXIF orientations map to LibRaw's `flip` codes (2→1, 3→3, 4→2, 5→4,
     6→6, 7→7, 8→5).
   - **Size:** the decode info's size is the upright one.
   - **What it records:** the orientation applied (`SourceInfo::orientation`;
     camera raws say 1). `PhotoMetadata` keeps the EXIF value.
   - **What follows:** everything after decode (editing, crops, masks, exports) sees
     the upright photo.
2. **A turn is a recipe-style transform** (`renderer::geometry::Turn`): mirrored first,
   then quarter turns clockwise, as `Geometry`'s `flip` and `rotation`. Every EXIF
   orientation is one (`Turn::from_exif`), and turns compose and invert as 2×2 signed
   permutations.
3. **Edits made before this keep their result.**
   - **Marking them:** recipe version 30 marks older recipes that edit something as
     `unoriented`, made on the file as stored.
   - **Adapting them** (`EditRecipe::on_upright`), where such a recipe meets the
     upright photo: on open, in exports, in thumbnails, when pasting onto a saved edit,
     and in any plan.
     - **Geometry:** the photo's turn is undone in its geometry: `rotation` and `flip`
       become the old ones composed with the inverse turn. The frame is therefore
       unchanged, so the crop, straighten, perspective and drawn masks (all in the
       frame) stay where they were.
     - **Spots and removals,** in the photo's own coordinates, move with the turn.
       Their sizes are fractions of the long edge or diagonal, so the turn leaves them
       alone.
     - **Generated masks,** made from the file as stored, are renamed missing. The
       editor makes them again from the upright photo (ADR 0074).
     - **The flag is cleared,** and the adapted recipe is saved with the next change.
   - **Edits that changed nothing** are no edit, like a photo never edited, and simply
     open upright.
   - **Copy and paste:** the flag travels with geometry.

## How it was checked

- **All eight orientations**, on a lossless PNG with an `eXIf` chunk: each upright
  pixel is where the EXIF standard puts it. The decode size, the library size and the
  thumbnail size agree.
- **`Turn::from_exif`** maps each stored pixel's centre to its upright place, for all
  eight.
- **Old edits:** an old edit (turned, cropped, straightened, a gradient mask, a heal
  spot) renders on the upright photo within 2 levels of how it rendered on the file
  as stored, for orientations 3, 6, 7 and 8.
- **Engine:** a JPEG recording orientation 6 opens upright (200 × 300 from a 300 × 200
  file). An old edit cropped to the stored file's left half still exports that half
  (150 × 200).

## Consequences

- Portrait photos from phones and cameras open upright as JPEG, PNG or TIFF.
- **Cost:** turning a 24 MP JPEG upright adds about 22 ms to its decode (46 → 68 ms).
- Recipe version 30.
