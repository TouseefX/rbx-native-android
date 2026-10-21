//! PCG32 (O'Neill's `pcg32_random_r`) seeded the way Luau's `Random.new`
//! seeds it: stream increment 105, and `NextNumber` built from two draws
//! (the second the high half). Checked against real Studio: `Random.new(7)`
//! gives 0.873708592, 0.432157216, 0.308992220, 0.107676387 there and here.
//! Documented (creator-docs `AnimationNodeType.yaml`, RandomSequenceNode):
//! Seed "ensur[es] the sequence remains consistent across clients" and
//! "defaults to `-1`, which uses a random seed". UNVERIFIED: how the engine
//! turns a node's Seed into its draws. Studio's picks for Seed 7 match no
//! `Random.new(seed)`-based draw tried (`NextNumber`, `NextInteger`, raw
//! draws modulo the input count, several seed and skip variants), and the
//! runtime lives in the engine, so here a seed simply starts this stream.

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    pub fn new(seed: u64) -> Pcg32 {
        let mut rng = Pcg32 {
            state: 0,
            inc: 105,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    pub fn next_u64(&mut self) -> u64 {
        (u64::from(self.next_u32()) << 32) | u64::from(self.next_u32())
    }

    /// Uniform in [0, 1), `Random:NextNumber()`.
    pub fn unit(&mut self) -> f32 {
        let low = u64::from(self.next_u32());
        let high = u64::from(self.next_u32());
        (((high << 32 | low) >> 11) as f64 / (1u64 << 53) as f64) as f32
    }

    /// Uniform in 0..n (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::Pcg32;

    #[test]
    fn seeds_like_luau_random() {
        let mut rng = Pcg32::new(7);
        for studio in [0.873_708_6_f32, 0.432_157_2, 0.308_992_2, 0.107_676_39] {
            assert!((rng.unit() - studio).abs() < 1e-6);
        }
    }
}
