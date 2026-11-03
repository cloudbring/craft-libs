use crate::Error;

/// Reduced exact rational with an i64 numerator and a strictly positive i64 denominator.
///
/// Arithmetic uses checked i128 intermediates and returns an error if the reduced result
/// does not fit. No operation wraps, saturates, or falls back to floating point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Ratio {
    numerator: i64,
    denominator: i64,
}

impl Ratio {
    /// Additive identity.
    pub const ZERO: Self = Self { numerator: 0, denominator: 1 };
    /// Multiplicative identity.
    pub const ONE: Self = Self { numerator: 1, denominator: 1 };

    /// Normalize a fraction; rejects zero denominators and unrepresentable normalized values.
    pub fn new(numerator: i64, denominator: i64) -> Result<Self, Error> {
        Self::from_wide(i128::from(numerator), i128::from(denominator))
    }

    /// Construct an integer without fallible arithmetic.
    pub const fn from_integer(value: i64) -> Self {
        Self { numerator: value, denominator: 1 }
    }

    /// Numerator in lowest terms.
    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    /// Positive denominator in lowest terms.
    pub const fn denominator(self) -> i64 {
        self.denominator
    }

    /// Approximate this value in f64. This conversion is not part of the exact arithmetic path.
    pub fn to_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    fn from_wide(numerator: i128, denominator: i128) -> Result<Self, Error> {
        if denominator == 0 {
            return Err(Error::ZeroDenominator);
        }
        let (numerator, denominator) = if denominator < 0 {
            (numerator.checked_neg().ok_or(Error::Overflow)?, denominator.checked_neg().ok_or(Error::Overflow)?)
        } else {
            (numerator, denominator)
        };
        let mut divisor = numerator.checked_abs().ok_or(Error::Overflow)?;
        let mut remainder = denominator;
        while remainder != 0 {
            (divisor, remainder) = (remainder, divisor % remainder);
        }
        Ok(Self {
            numerator: i64::try_from(numerator / divisor).map_err(|_| Error::Overflow)?,
            denominator: i64::try_from(denominator / divisor).map_err(|_| Error::Overflow)?,
        })
    }

    /// Exact sum, or [`Error::Overflow`].
    pub fn checked_add(self, other: Self) -> Result<Self, Error> {
        let left = i128::from(self.numerator) * i128::from(other.denominator);
        let right = i128::from(other.numerator) * i128::from(self.denominator);
        Self::from_wide(left.checked_add(right).ok_or(Error::Overflow)?, i128::from(self.denominator) * i128::from(other.denominator))
    }

    /// Exact difference, or [`Error::Overflow`].
    pub fn checked_sub(self, other: Self) -> Result<Self, Error> {
        let left = i128::from(self.numerator) * i128::from(other.denominator);
        let right = i128::from(other.numerator) * i128::from(self.denominator);
        Self::from_wide(left.checked_sub(right).ok_or(Error::Overflow)?, i128::from(self.denominator) * i128::from(other.denominator))
    }

    /// Exact product, or [`Error::Overflow`].
    pub fn checked_mul(self, other: Self) -> Result<Self, Error> {
        Self::from_wide(i128::from(self.numerator) * i128::from(other.numerator), i128::from(self.denominator) * i128::from(other.denominator))
    }

    /// Exact quotient, or [`Error::ZeroDenominator`] / [`Error::Overflow`].
    pub fn checked_div(self, other: Self) -> Result<Self, Error> {
        Self::from_wide(i128::from(self.numerator) * i128::from(other.denominator), i128::from(self.denominator) * i128::from(other.numerator))
    }

    pub(crate) fn sqrt_exact(self) -> Result<Self, Error> {
        let numerator = u64::try_from(self.numerator).map_err(|_| Error::Irrational)?;
        let denominator = u64::try_from(self.denominator).map_err(|_| Error::Irrational)?;
        let numerator_root = numerator.isqrt();
        let denominator_root = denominator.isqrt();
        if numerator_root * numerator_root != numerator || denominator_root * denominator_root != denominator {
            return Err(Error::Irrational);
        }
        Self::from_wide(i128::from(numerator_root), i128::from(denominator_root))
    }
}

impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator)).cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
}

impl PartialOrd for Ratio {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::fmt::Display for Ratio {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}
