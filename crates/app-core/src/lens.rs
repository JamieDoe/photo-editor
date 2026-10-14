//! Lens corrections from the photo's own profile (ADR 0075), read from its file.

use std::path::Path;

use renderer::lens::LensCorrection;

/// The lens corrections `path` records, ready for the renderer, and the lens's name
/// (or, without one, who recorded them: "Sony"). Reads a few kilobytes of the file's
/// headers.
pub(crate) fn of(path: &Path) -> Option<(LensCorrection, String)> {
    let profile = raw::lens::read(path)?;
    let correction = LensCorrection::new(
        &profile.knots,
        profile.distortion.as_deref(),
        profile.vignetting.as_deref(),
        profile
            .chromatic_aberration
            .as_ref()
            .map(|[red, blue]| [&red[..], &blue[..]]),
    )?;
    Some((
        correction,
        profile.lens.unwrap_or_else(|| profile.source.to_owned()),
    ))
}

/// `lens` as `recipe` applies the photo's profile: not at all when it is off, and
/// without its chromatic aberration for edits made before profiles corrected it (ADR
/// 0083).
pub(crate) fn applied(
    recipe: &renderer::EditRecipe,
    lens: Option<LensCorrection>,
) -> Option<LensCorrection> {
    let lens = lens.filter(|_| recipe.profile_corrections)?;
    if recipe.profile_without_chromatic_aberration {
        lens.without_chromatic_aberration()
    } else {
        Some(lens)
    }
}
