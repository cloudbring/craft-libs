//! SplitMix64: a tiny seeded generator. Every random choice in the harness (schedule order,
//! bootstrap resampling) comes from here so a report can be reproduced from its seed.

/// SplitMix64 (Steele, Lea, Flood 2014).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (multiply-shift; bias below 2^-64 * n). `n` must be non-zero.
    pub fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }

    /// Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

/// Derives an independent stream seed from a base seed, an index and a salt.
pub fn derive(seed: u64, index: usize, salt: u64) -> u64 {
    let mut r = Rng::new(seed ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    for _ in 0..=index.min(4096) {
        r.next_u64();
    }
    r.next_u64() ^ (index as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_known_vector() {
        // Reference values for seed 0 (Vigna's splitmix64.c).
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn below_in_range_and_shuffle_is_permutation() {
        let mut r = Rng::new(7);
        assert!((0..1000).all(|_| r.below(13) < 13));
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        let mut s = v.clone();
        s.sort_unstable();
        assert_eq!(s, (0..50).collect::<Vec<_>>());
        assert_ne!(v, s);
    }
}
