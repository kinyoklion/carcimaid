//! Port of rough.js `math.js` (the `Random` PRNG).
//!
//! rough.js `Random.next()`:
//! ```js
//! next() {
//!   if (this.seed) {
//!     return ((2 ** 31 - 1) & (this.seed = Math.imul(48271, this.seed))) / 2 ** 31;
//!   } else {
//!     return Math.random();
//!   }
//! }
//! ```
//!
//! Deviation from rough.js: rough.js falls back to the non-deterministic
//! `Math.random()` when `seed === 0` (0 is falsy). We instead ALWAYS run the
//! Lehmer / Park-Miller LCG. With `seed = 0` this yields a degenerate all-zero
//! sequence (`48271 * 0 == 0`), which is deterministic. Our `Generator` defaults
//! `seed` to `1`, so the LCG runs with a real state and output is stable.

/// A minimal xorshift-free Lehmer LCG matching rough.js `Random`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Random {
    /// 32-bit signed state, matching JS `Math.imul` semantics.
    pub seed: i32,
}

impl Random {
    #[inline]
    pub fn new(seed: i32) -> Self {
        Random { seed }
    }

    /// Equivalent to rough.js `Random.next()`.
    ///
    /// `Math.imul(48271, seed)` maps directly onto `i32::wrapping_mul`, and
    /// `(2**31 - 1) & seed` onto a bitwise AND with `0x7FFF_FFFF` (which also
    /// clears the sign bit, yielding a value in `[0, 2**31)`).
    #[inline]
    #[allow(clippy::should_implement_trait)] // faithful to rough.js `Random.next()`
    pub fn next(&mut self) -> f64 {
        self.seed = 48271i32.wrapping_mul(self.seed);
        ((0x7FFF_FFFFi32 & self.seed) as f64) / 2147483648.0 // 2**31
    }
}

/// Port of rough.js `randomSeed()` — `Math.floor(Math.random() * 2 ** 31)`.
///
/// This crate ships no RNG dependency, so this is intentionally a deterministic
/// stub returning a fixed non-zero seed. Callers wanting real randomness should
/// supply their own seed. (rough.js only uses this when the caller does not set
/// a seed; we deliberately default to a fixed seed for determinism.)
pub fn random_seed() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_one_sequence_matches_hand_computed() {
        // Computed independently with JS-exact 32-bit integer semantics:
        //   seed = imul(48271, seed); v = ((2**31-1) & seed) / 2**31
        let mut r = Random::new(1);
        let expected = [
            2.2477935999631882e-05,
            0.08503244863823056,
            0.6013282160274684,
            0.714315861929208,
            0.7409711848013103,
            0.4200615440495312,
        ];
        for (i, e) in expected.iter().enumerate() {
            let v = r.next();
            assert_eq!(v, *e, "mismatch at index {i}");
        }
    }

    #[test]
    fn seed_state_wraps_like_math_imul() {
        let mut r = Random::new(1);
        r.next();
        assert_eq!(r.seed, 48271);
        r.next();
        assert_eq!(r.seed, -1964877855);
    }
}
