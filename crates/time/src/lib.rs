//! Exact rational media time and SMPTE drop-frame timecode.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::fmt;

/// Invalid input or a result outside the exact storage range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeError {
    ZeroDenominator,
    InvalidFrameRate,
    InvalidTimecode,
    UnsupportedDropFrame,
    Overflow,
}

impl fmt::Display for TimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TimeError {}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

/// An exact number of seconds, always in lowest terms with a positive denominator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RationalTime {
    numerator: i64,
    denominator: u32,
}

/// A duration in exact seconds.
pub type Duration = RationalTime;

impl Default for RationalTime {
    fn default() -> Self {
        Self::ZERO
    }
}

impl RationalTime {
    pub const ZERO: Self = Self { numerator: 0, denominator: 1 };

    pub fn new(numerator: i64, denominator: u32) -> Result<Self, TimeError> {
        Self::reduced(i128::from(numerator), u128::from(denominator))
    }

    fn reduced(numerator: i128, denominator: u128) -> Result<Self, TimeError> {
        if denominator == 0 {
            return Err(TimeError::ZeroDenominator);
        }
        let divisor = gcd(numerator.unsigned_abs(), denominator);
        let numerator = numerator / i128::try_from(divisor).map_err(|_| TimeError::Overflow)?;
        let denominator = denominator / divisor;
        Ok(Self {
            numerator: i64::try_from(numerator).map_err(|_| TimeError::Overflow)?,
            denominator: u32::try_from(denominator).map_err(|_| TimeError::Overflow)?,
        })
    }

    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        let numerator = i128::from(self.numerator) * i128::from(other.denominator) + i128::from(other.numerator) * i128::from(self.denominator);
        Self::reduced(numerator, u128::from(self.denominator) * u128::from(other.denominator))
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
        let numerator = i128::from(self.numerator) * i128::from(other.denominator) - i128::from(other.numerator) * i128::from(self.denominator);
        Self::reduced(numerator, u128::from(self.denominator) * u128::from(other.denominator))
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, TimeError> {
        Self::reduced(i128::from(self.numerator) * i128::from(other.numerator), u128::from(self.denominator) * u128::from(other.denominator))
    }

    /// Exact sample position divided by the sample rate.
    pub fn from_samples(position: i64, sample_rate: u32) -> Result<Self, TimeError> {
        Self::new(position, sample_rate)
    }

    /// Sample position at or before this time; negative positions floor toward minus infinity.
    pub fn to_samples_floor(self, sample_rate: u32) -> Result<i64, TimeError> {
        if sample_rate == 0 {
            return Err(TimeError::ZeroDenominator);
        }
        i64::try_from((i128::from(self.numerator) * i128::from(sample_rate)).div_euclid(i128::from(self.denominator)))
            .map_err(|_| TimeError::Overflow)
    }
}

/// Physical frames per second as a reduced positive fraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl FrameRate {
    pub const FPS_23_976: Self = Self { numerator: 24_000, denominator: 1001 };
    pub const FPS_24: Self = Self { numerator: 24, denominator: 1 };
    pub const FPS_25: Self = Self { numerator: 25, denominator: 1 };
    pub const FPS_29_97: Self = Self { numerator: 30_000, denominator: 1001 };
    pub const FPS_30: Self = Self { numerator: 30, denominator: 1 };
    pub const FPS_48: Self = Self { numerator: 48, denominator: 1 };
    pub const FPS_50: Self = Self { numerator: 50, denominator: 1 };
    pub const FPS_59_94: Self = Self { numerator: 60_000, denominator: 1001 };
    pub const FPS_60: Self = Self { numerator: 60, denominator: 1 };
    pub const FPS_119_88: Self = Self { numerator: 120_000, denominator: 1001 };
    pub const FPS_120: Self = Self { numerator: 120, denominator: 1 };

    pub fn new(numerator: u32, denominator: u32) -> Result<Self, TimeError> {
        if numerator == 0 || denominator == 0 {
            return Err(TimeError::InvalidFrameRate);
        }
        let divisor = gcd(u128::from(numerator), u128::from(denominator)) as u32;
        Ok(Self { numerator: numerator / divisor, denominator: denominator / divisor })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Nominal timecode frames per second (FilmCraft's ceiling convention).
    pub fn timecode_base(self) -> u32 {
        self.numerator / self.denominator + u32::from(!self.numerator.is_multiple_of(self.denominator))
    }

    pub fn supports_drop_frame(self) -> bool {
        self.denominator == 1001 && self.timecode_base().is_multiple_of(30)
    }

    /// Exact frame boundary in seconds.
    pub fn time_of_frame(self, frame: i64) -> Result<RationalTime, TimeError> {
        RationalTime::reduced(i128::from(frame) * i128::from(self.denominator), u128::from(self.numerator))
    }

    /// Frame containing this instant, flooring toward minus infinity.
    pub fn frame_at(self, time: RationalTime) -> Result<i64, TimeError> {
        i64::try_from(
            (i128::from(time.numerator) * i128::from(self.numerator)).div_euclid(i128::from(time.denominator) * i128::from(self.denominator)),
        )
        .map_err(|_| TimeError::Overflow)
    }
}

/// A validated, signed timecode label; DF skips labels, never physical frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timecode {
    rate: FrameRate,
    drop_frame: bool,
    negative: bool,
    hours: u32,
    minutes: u8,
    seconds: u8,
    frames: u32,
}

impl Timecode {
    pub fn new(rate: FrameRate, drop_frame: bool, negative: bool, hours: u32, minutes: u8, seconds: u8, frames: u32) -> Result<Self, TimeError> {
        if drop_frame && !rate.supports_drop_frame() {
            return Err(TimeError::UnsupportedDropFrame);
        }
        if minutes >= 60 || seconds >= 60 || frames >= rate.timecode_base() || (negative && hours == 0 && minutes == 0 && seconds == 0 && frames == 0)
        {
            return Err(TimeError::InvalidTimecode);
        }
        let skipped = rate.timecode_base() / 15;
        if drop_frame && !minutes.is_multiple_of(10) && seconds == 0 && frames < skipped {
            return Err(TimeError::InvalidTimecode);
        }
        Ok(Self { rate, drop_frame, negative, hours, minutes, seconds, frames })
    }

    pub const fn fields(self) -> (bool, u32, u8, u8, u32) {
        (self.negative, self.hours, self.minutes, self.seconds, self.frames)
    }

    pub const fn rate(self) -> FrameRate {
        self.rate
    }

    pub const fn is_drop_frame(self) -> bool {
        self.drop_frame
    }

    /// FilmCraft's ten-minute-block forward mapping, widened to i128 before arithmetic.
    pub fn from_frames(frame: i64, rate: FrameRate, drop_frame: bool) -> Result<Self, TimeError> {
        if drop_frame && !rate.supports_drop_frame() {
            return Err(TimeError::UnsupportedDropFrame);
        }
        let negative = frame < 0;
        let mut label = i128::from(frame).abs();
        let base = i128::from(rate.timecode_base());
        let drop = base / 15;
        if drop_frame {
            let frames_per_minute = base * 60 - drop;
            let frames_per_ten_minutes = frames_per_minute * 10 + drop;
            let ten_minute_blocks = label / frames_per_ten_minutes;
            let remainder = label % frames_per_ten_minutes;
            label += drop * 9 * ten_minute_blocks;
            if remainder > drop {
                label += drop * ((remainder - drop) / frames_per_minute);
            }
        }
        let hours = u32::try_from(label / (base * 3600)).map_err(|_| TimeError::Overflow)?;
        let minutes = u8::try_from((label / (base * 60)) % 60).map_err(|_| TimeError::Overflow)?;
        let seconds = u8::try_from((label / base) % 60).map_err(|_| TimeError::Overflow)?;
        let frames = u32::try_from(label % base).map_err(|_| TimeError::Overflow)?;
        Self::new(rate, drop_frame, negative, hours, minutes, seconds, frames)
    }

    /// FilmCraft's inverse: subtract skipped labels before this minute.
    pub fn to_frames(self) -> Result<i64, TimeError> {
        let base = i128::from(self.rate.timecode_base());
        let total_minutes = i128::from(self.hours) * 60 + i128::from(self.minutes);
        let seconds = total_minutes * 60 + i128::from(self.seconds);
        let mut frame = seconds * base + i128::from(self.frames);
        if self.drop_frame {
            let drop = base / 15;
            frame -= drop * (total_minutes - total_minutes / 10);
        }
        if self.negative {
            frame = -frame;
        }
        i64::try_from(frame).map_err(|_| TimeError::Overflow)
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.negative { "-" } else { "" };
        let separator = if self.drop_frame { ';' } else { ':' };
        write!(formatter, "{sign}{:02}:{:02}:{:02}{separator}{:02}", self.hours, self.minutes, self.seconds, self.frames)
    }
}

#[cfg(test)]
mod tests;
