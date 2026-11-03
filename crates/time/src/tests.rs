use super::{FrameRate, RationalTime, TimeError, Timecode};
use proptest::prelude::*;

#[test]
fn known_smpte_vectors_and_boundaries() {
    for (rate, minute, ten_minutes, day) in [(FrameRate::FPS_29_97, 1_800, 17_982, 2_589_408), (FrameRate::FPS_59_94, 3_600, 35_964, 5_178_816)] {
        let drop = rate.timecode_base() / 15;
        assert_eq!(Timecode::from_frames(minute, rate, true).map(|tc| tc.fields()), Ok((false, 0, 1, 0, drop)));
        assert_eq!(Timecode::from_frames(ten_minutes, rate, true).map(|tc| tc.fields()), Ok((false, 0, 10, 0, 0)));
        assert_eq!(Timecode::from_frames(day - 1, rate, true).and_then(|tc| tc.to_frames()), Ok(day - 1));
        assert_eq!(Timecode::from_frames(day, rate, true).map(|tc| tc.fields()), Ok((false, 24, 0, 0, 0)));
        assert_eq!(Timecode::new(rate, true, false, 0, 1, 0, drop - 1), Err(TimeError::InvalidTimecode));
        assert!(Timecode::new(rate, true, false, 0, 10, 0, 0).is_ok());
        assert_eq!(Timecode::from_frames(-minute, rate, true).and_then(|tc| tc.to_frames()), Ok(-minute));
    }
    assert_eq!(Timecode::from_frames(1800, FrameRate::FPS_29_97, true).map(|tc| tc.to_string()), Ok("00:01:00;02".into()));
    assert_eq!(Timecode::from_frames(17_982, FrameRate::FPS_29_97, true).map(|tc| tc.to_string()), Ok("00:10:00;00".into()));
    assert_eq!(Timecode::from_frames(3600, FrameRate::FPS_59_94, true).map(|tc| tc.to_string()), Ok("00:01:00;04".into()));
    assert_eq!(Timecode::from_frames(35_964, FrameRate::FPS_59_94, true).map(|tc| tc.to_string()), Ok("00:10:00;00".into()));
}

#[test]
fn exhaustive_drop_frame_day_bijection() {
    for (rate, frames_per_day) in [(FrameRate::FPS_29_97, 2_589_408), (FrameRate::FPS_59_94, 5_178_816)] {
        for frame in 0..frames_per_day {
            let result = Timecode::from_frames(frame, rate, true);
            assert!(result.is_ok(), "frame {frame}");
            let Ok(timecode) = result else { continue };
            assert_eq!(timecode.to_frames(), Ok(frame), "{timecode}");
        }
    }
}

#[test]
fn rational_frames_samples_and_validation() {
    assert_eq!(RationalTime::default(), RationalTime::ZERO);
    assert_eq!(RationalTime::new(48_000, 48_000), RationalTime::new(1, 1));
    assert_eq!(FrameRate::new(60_000, 2002), Ok(FrameRate::FPS_29_97));
    assert_eq!(FrameRate::new(37_516, 1000), Ok(FrameRate { numerator: 9379, denominator: 250 }));
    assert_eq!(FrameRate::new(0, 1), Err(TimeError::InvalidFrameRate));
    assert_eq!(RationalTime::new(1, 0), Err(TimeError::ZeroDenominator));
    assert_eq!(Timecode::new(FrameRate::FPS_24, false, true, 0, 0, 0, 0), Err(TimeError::InvalidTimecode));
    assert_eq!(Timecode::from_frames(0, FrameRate::FPS_24, true), Err(TimeError::UnsupportedDropFrame));
    assert_eq!(RationalTime::from_samples(-1, 192_000).and_then(|t| t.to_samples_floor(192_000)), Ok(-1));
    assert_eq!(RationalTime::new(-1, 2).and_then(|t| t.to_samples_floor(3)), Ok(-2));
    assert_eq!(FrameRate::FPS_23_976.time_of_frame(24_000), RationalTime::new(1001, 1));
    assert_eq!(FrameRate::FPS_23_976.time_of_frame(-1).and_then(|t| FrameRate::FPS_23_976.frame_at(t)), Ok(-1));
    assert_eq!(RationalTime::new(i64::MAX, 1).and_then(|t| t.checked_add(t)), Err(TimeError::Overflow));
    for rate in [
        FrameRate::FPS_23_976,
        FrameRate::FPS_24,
        FrameRate::FPS_25,
        FrameRate::FPS_29_97,
        FrameRate::FPS_30,
        FrameRate::FPS_48,
        FrameRate::FPS_50,
        FrameRate::FPS_59_94,
        FrameRate::FPS_60,
        FrameRate::FPS_119_88,
        FrameRate::FPS_120,
        FrameRate { numerator: 9379, denominator: 250 },
    ] {
        for frame in [-240, -1, 0, 1, 240, 100_000] {
            assert_eq!(rate.time_of_frame(frame).and_then(|time| rate.frame_at(time)), Ok(frame));
            assert_eq!(Timecode::from_frames(frame, rate, false).and_then(|tc| tc.to_frames()), Ok(frame));
        }
    }
}

#[test]
fn app_overflow_behavior_diverges() {
    let rate = original::FrameRate { num: 30_000, den: 1001 };
    assert_ne!(filmcraft::frames_to_fields(i64::MIN, rate, true), effectcraft::frames_to_fields(i64::MIN, rate, true));
    assert_ne!(filmcraft::fields_to_frames(i64::MAX, 0, 0, 0, rate, true), effectcraft::fields_to_frames(i64::MAX, 0, 0, 0, rate, true));
}

proptest! {
    #[test]
    fn rational_addition_associative_and_commutative(
        left in -1_000_000i64..1_000_000,
        middle in -1_000_000i64..1_000_000,
        right in -1_000_000i64..1_000_000,
        left_den in 1u32..241,
        middle_den in 1u32..241,
        right_den in 1u32..241,
    ) {
        let left = RationalTime::new(left, left_den)?;
        let middle = RationalTime::new(middle, middle_den)?;
        let right = RationalTime::new(right, right_den)?;
        prop_assert_eq!(left.checked_add(middle)?, middle.checked_add(left)?);
        prop_assert_eq!(left.checked_add(middle)?.checked_add(right)?, left.checked_add(middle.checked_add(right)?)?);
        prop_assert_eq!(left.checked_mul(middle)?, middle.checked_mul(left)?);
        prop_assert_eq!(left.checked_mul(middle)?.checked_mul(right)?, left.checked_mul(middle.checked_mul(right)?)?);
        prop_assert_eq!(left.checked_sub(middle)?.checked_add(middle)?, left);
    }

    #[test]
    fn sample_position_is_exact_within_24_hours(position in 0i64..(24 * 60 * 60 * 192_000)) {
        let time = RationalTime::from_samples(position, 192_000)?;
        prop_assert_eq!(time.to_samples_floor(192_000)?, position);
        let next = time.checked_add(RationalTime::from_samples(1, 192_000)?)?;
        prop_assert_eq!(next.to_samples_floor(192_000)?, position + 1);
    }
}

#[test]
fn differential_against_both_app_implementations() {
    for rate in [FrameRate::FPS_24, FrameRate::FPS_29_97, FrameRate::FPS_59_94, FrameRate::FPS_60] {
        for drop_frame in [false, true] {
            if drop_frame && !rate.supports_drop_frame() {
                continue;
            }
            for frame in [-1_000_000, -1800, -1, 0, 1, 1799, 1800, 3600, 17_982, 35_964, 2_589_407, 5_178_815, 9_999_999] {
                let result = Timecode::from_frames(frame, rate, drop_frame);
                assert!(result.is_ok(), "frame {frame}");
                let Ok(timecode) = result else { continue };
                let original_rate = original::FrameRate { num: i64::from(rate.numerator()), den: i64::from(rate.denominator()) };
                let expected = filmcraft::frames_to_fields(frame, original_rate, drop_frame);
                assert_eq!(timecode.fields(), (expected.0, expected.1 as u32, expected.2 as u8, expected.3 as u8, expected.4 as u32));
                assert_eq!(effectcraft::frames_to_fields(frame, original_rate, drop_frame), expected);
                let (negative, hours, minutes, seconds, frames) = timecode.fields();
                let positive = filmcraft::fields_to_frames(
                    i64::from(hours),
                    i64::from(minutes),
                    i64::from(seconds),
                    i64::from(frames),
                    original_rate,
                    drop_frame,
                );
                assert_eq!(
                    effectcraft::fields_to_frames(
                        i64::from(hours),
                        i64::from(minutes),
                        i64::from(seconds),
                        i64::from(frames),
                        original_rate,
                        drop_frame
                    ),
                    positive
                );
                assert_eq!(timecode.to_frames(), Ok(if negative { -positive } else { positive }));
            }
        }
    }
}

mod original {
    #[derive(Clone, Copy)]
    pub struct FrameRate {
        pub num: i64,
        pub den: i64,
    }

    impl FrameRate {
        pub fn timecode_base(self) -> i64 {
            (self.num / self.den).saturating_add(i64::from(self.num % self.den != 0)).max(1)
        }

        pub fn supports_drop_frame(self) -> bool {
            self.den == 1001 && self.timecode_base() % 30 == 0
        }
    }
}

mod filmcraft {
    //! Verbatim drop-frame functions from /Users/emm/ArtCraft/filmcraft/crates/time/src/lib.rs:372-412.
    use super::original::FrameRate;

    fn clamp_i64(value: i128) -> i64 {
        value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    fn df_params(rate: FrameRate) -> (i64, i64) {
        let base = rate.timecode_base();
        (base / 15, base) // 30 → 2, 60 → 4, 120 → 8
    }

    pub fn frames_to_fields(frame: i64, rate: FrameRate, drop_frame: bool) -> (bool, i64, i64, i64, i64) {
        let neg = frame < 0;
        let mut n = i128::from(frame).abs();
        let (drop, base) = df_params(rate);
        let (drop, base) = (i128::from(drop), i128::from(base));
        if drop_frame && rate.supports_drop_frame() {
            let per_min = base * 60 - drop;
            let per_10 = per_min * 10 + drop;
            let d = n / per_10;
            let m = n % per_10;
            n = n.saturating_add(drop * 9 * d);
            if m > drop {
                n = n.saturating_add(drop * ((m - drop) / per_min));
            }
        }
        let f = n % base;
        let s = (n / base) % 60;
        let mi = (n / (base * 60)) % 60;
        let h = n / (base * 3600);
        (neg, clamp_i64(h), clamp_i64(mi), clamp_i64(s), clamp_i64(f))
    }

    pub fn fields_to_frames(h: i64, m: i64, s: i64, f: i64, rate: FrameRate, drop_frame: bool) -> i64 {
        let (drop, base) = df_params(rate);
        let seconds = i128::from(h) * 3600 + i128::from(m) * 60 + i128::from(s);
        let mut n = seconds.saturating_mul(i128::from(base)).saturating_add(i128::from(f));
        if drop_frame && rate.supports_drop_frame() {
            let total_min = i128::from(h) * 60 + i128::from(m);
            n = n.saturating_sub(i128::from(drop) * (total_min - total_min / 10));
        }
        clamp_i64(n)
    }
}

mod effectcraft {
    //! Verbatim drop-frame functions from /Users/emm/ArtCraft/effectcraft/crates/time/src/lib.rs:314-352.
    use super::original::FrameRate;

    fn df_params(rate: FrameRate) -> (i64, i64) {
        let base = rate.timecode_base();
        (base / 15, base) // 30 → 2, 60 → 4, 120 → 8
    }

    pub fn frames_to_fields(frame: i64, rate: FrameRate, drop_frame: bool) -> (bool, i64, i64, i64, i64) {
        let neg = frame < 0;
        let mut n = frame.saturating_abs();
        let (drop, base) = df_params(rate);
        if drop_frame && rate.supports_drop_frame() {
            let per_min = base.saturating_mul(60) - drop;
            let per_10 = per_min.saturating_mul(10) + drop;
            let d = n / per_10;
            let m = n % per_10;
            n = n.saturating_add((drop * 9).saturating_mul(d));
            if m > drop {
                n = n.saturating_add(drop * ((m - drop) / per_min));
            }
        }
        let f = n % base;
        let s = (n / base) % 60;
        let mi = (n / base.saturating_mul(60)) % 60;
        let h = n / base.saturating_mul(3600);
        (neg, h, mi, s, f)
    }

    pub fn fields_to_frames(h: i64, m: i64, s: i64, f: i64, rate: FrameRate, drop_frame: bool) -> i64 {
        let (drop, base) = df_params(rate);
        let secs = h.saturating_mul(3600).saturating_add(m.saturating_mul(60)).saturating_add(s);
        let mut n = secs.saturating_mul(base).saturating_add(f);
        if drop_frame && rate.supports_drop_frame() {
            let total_min = h.saturating_mul(60).saturating_add(m);
            n = n.saturating_sub(drop.saturating_mul(total_min - total_min / 10));
        }
        n
    }
}
