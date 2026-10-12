//! A small fixed pseudo-random generator. It lives here rather than in a
//! dependency so that the generated data never changes with a crate update.

/// SplitMix64.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, or 0 when `n` is 0. The slight modulo bias does
    /// not matter here.
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// A number in `lo..=hi`.
    pub(crate) fn between(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi.saturating_sub(lo) + 1)
    }

    /// True with a probability of `per_mille` / 1000.
    pub(crate) fn chance(&mut self, per_mille: u64) -> bool {
        self.below(1000) < per_mille
    }
}

/// A stateless hash of a few numbers, for values that must not depend on the
/// order in which they are generated.
pub(crate) fn mix(values: &[u64]) -> u64 {
    let mut rng = Rng::new(0x005E_ED0F_C40C);
    let mut hash = 0;
    for value in values {
        rng.0 ^= value.wrapping_add(hash);
        hash = rng.next_u64();
    }
    hash
}

/// A stable 64-bit hash of a string (FNV-1a).
pub(crate) fn hash_str(text: &str) -> u64 {
    text.bytes().fold(0xCBF2_9CE4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3)
    })
}
