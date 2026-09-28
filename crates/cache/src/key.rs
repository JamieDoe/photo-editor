use crate::hash::Fnv64;

/// Stable identity of a source photograph's *content version*.
///
/// Computed by the caller from file identity (size, modification time and a content
/// fingerprint). Deliberately not derived from the path alone, so a moved file can be
/// recognised and an edited-in-place file is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u64);

/// Key for a rendered preview.
///
/// Accounts for every input that changes the output pixels: source content, the edit
/// recipe, the rendered resolution/format, and the renderer version (so a renderer
/// change never serves stale pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderKey {
    pub source: SourceId,
    pub recipe: u64,
    pub width: u32,
    pub height: u32,
    pub format: u8,
    pub renderer_version: u32,
}

impl RenderKey {
    /// `recipe_bytes` must be the recipe's canonical serialisation.
    pub fn new(
        source: SourceId,
        recipe_bytes: &[u8],
        width: u32,
        height: u32,
        format: u8,
        renderer_version: u32,
    ) -> Self {
        Self {
            source,
            recipe: Fnv64::new().write(recipe_bytes).finish(),
            width,
            height,
            format,
            renderer_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(recipe: &[u8], w: u32, version: u32) -> RenderKey {
        RenderKey::new(SourceId(1), recipe, w, 100, 0, version)
    }

    #[test]
    fn identical_inputs_produce_identical_keys() {
        assert_eq!(
            key(b"{\"exposure\":1}", 200, 1),
            key(b"{\"exposure\":1}", 200, 1)
        );
    }

    #[test]
    fn every_input_changes_the_key() {
        let base = key(b"r", 200, 1);
        assert_ne!(base, key(b"r2", 200, 1), "recipe");
        assert_ne!(base, key(b"r", 201, 1), "resolution");
        assert_ne!(base, key(b"r", 200, 2), "renderer version");
        assert_ne!(
            base,
            RenderKey::new(SourceId(2), b"r", 200, 100, 0, 1),
            "source"
        );
        assert_ne!(
            base,
            RenderKey::new(SourceId(1), b"r", 200, 100, 1, 1),
            "format"
        );
    }
}
