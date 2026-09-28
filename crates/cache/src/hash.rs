/// FNV-1a 64-bit hasher.
///
/// Used for cache keys because, unlike `std`'s `DefaultHasher`, its output is stable
/// across Rust versions and processes, so keys can later back an on-disk cache.
/// It is not collision-resistant against adversarial input; that is acceptable for
/// cache keys over local data.
#[derive(Debug, Clone, Copy)]
pub struct Fnv64(u64);

impl Fnv64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub fn new() -> Self {
        Self(Self::OFFSET)
    }

    pub fn write(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
        self
    }

    pub fn write_u64(&mut self, v: u64) -> &mut Self {
        self.write(&v.to_le_bytes())
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    Fnv64::new().write(bytes).finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn incremental_equals_one_shot() {
        let mut h = Fnv64::new();
        h.write(b"foo").write(b"bar");
        assert_eq!(h.finish(), fnv1a64(b"foobar"));
    }
}
