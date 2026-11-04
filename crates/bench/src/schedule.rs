//! The alternating run schedule.
//!
//! Each input gets `ceil(repetitions / 2)` blocks. A block is four runs, `ABBA` or `BAAB`
//! (A = old, B = new), the start chosen at random. Blocks of all inputs are then shuffled, so
//! slow drift (thermal, background load) hits old and new alike and no input owns a time slot.
//! Each side runs `2 * blocks` times per input, which is `repetitions` rounded up to even.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;

/// Which implementation a run measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Old,
    New,
}

/// One timed run: which input, which side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub input: usize,
    pub side: Side,
}

/// Runs per side per input for a requested repetition count.
pub const fn runs_per_side(repetitions: usize) -> usize {
    repetitions.div_ceil(2) * 2
}

/// Builds the schedule. Deterministic in `seed`.
pub fn build(inputs: usize, repetitions: usize, seed: u64) -> Vec<Step> {
    let blocks_per_input = repetitions.div_ceil(2);
    let mut rng = Rng::new(seed);
    let mut blocks: Vec<(usize, bool)> = Vec::with_capacity(inputs * blocks_per_input);
    for input in 0..inputs {
        for _ in 0..blocks_per_input {
            blocks.push((input, rng.next_u64() & 1 == 0));
        }
    }
    rng.shuffle(&mut blocks);
    let mut steps = Vec::with_capacity(blocks.len() * 4);
    for (input, old_first) in blocks {
        let (a, b) = if old_first { (Side::Old, Side::New) } else { (Side::New, Side::Old) };
        steps.extend([a, b, b, a].map(|side| Step { input, side }));
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(steps: &[Step], input: usize, side: Side) -> usize {
        steps.iter().filter(|s| s.input == input && s.side == side).count()
    }

    #[test]
    fn counts_are_balanced_per_input() {
        for reps in [10, 11, 20] {
            let s = build(3, reps, 5);
            for i in 0..3 {
                assert_eq!(count(&s, i, Side::Old), runs_per_side(reps));
                assert_eq!(count(&s, i, Side::New), runs_per_side(reps));
            }
        }
        assert_eq!(runs_per_side(10), 10);
        assert_eq!(runs_per_side(11), 12);
    }

    #[test]
    fn every_block_alternates_abba_or_baab_and_runs_never_exceed_two() {
        let s = build(4, 10, 99);
        for block in s.chunks(4) {
            assert!(block.iter().all(|x| x.input == block[0].input));
            let sides: Vec<Side> = block.iter().map(|x| x.side).collect();
            let abba = [Side::Old, Side::New, Side::New, Side::Old];
            let baab = [Side::New, Side::Old, Side::Old, Side::New];
            assert!(sides == abba || sides == baab, "{sides:?}");
        }
        let mut run = 1;
        for w in s.windows(2) {
            run = if w[0].side == w[1].side { run + 1 } else { 1 };
            assert!(run <= 2, "three consecutive runs of one side");
        }
    }

    #[test]
    fn start_is_randomised_and_seeded() {
        let a = build(2, 10, 1);
        assert_eq!(a, build(2, 10, 1));
        assert_ne!(a, build(2, 10, 2));
        let old_first = a.chunks(4).filter(|b| b[0].side == Side::Old).count();
        assert!(old_first > 0 && old_first < a.len() / 4, "start is not randomised");
    }
}
