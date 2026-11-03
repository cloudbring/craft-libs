#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod ratio;

pub use ratio::Ratio;

/// The twelve separable blend modes in W3C Compositing and Blending Level 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Use the source channel.
    Normal,
    /// Multiply the two channels.
    Multiply,
    /// Complement of multiplying complements.
    Screen,
    /// Hard light with source and backdrop exchanged.
    Overlay,
    /// Minimum of the channels.
    Darken,
    /// Maximum of the channels.
    Lighten,
    /// Divide the backdrop by the source complement, with the spec's endpoint precedence.
    ColorDodge,
    /// Complement of dividing the backdrop complement by the source.
    ColorBurn,
    /// Multiply or screen according to the source.
    HardLight,
    /// Piecewise polynomial / square-root soft light, not the older Photoshop approximation.
    SoftLight,
    /// Absolute channel difference.
    Difference,
    /// Sum minus twice the product.
    Exclusion,
}

/// A single straight (unassociated) colour channel and its alpha, both in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel<T> {
    /// Straight colour, not multiplied by alpha.
    pub color: T,
    /// Alpha, including any coverage / opacity already applied by the caller.
    pub alpha: T,
}

/// Explicit failures; invalid inputs are never silently clamped or approximated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A channel or alpha is non-finite or outside `[0, 1]`.
    OutOfRange,
    /// A rational constructor or division has a zero denominator.
    ZeroDenominator,
    /// A normalized intermediate cannot be represented by the bounded rational type.
    Overflow,
    /// SoftLight requires a square root not representable as a rational number.
    Irrational,
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::OutOfRange => "colour and alpha must be finite and in [0, 1]",
            Self::ZeroDenominator => "zero rational denominator",
            Self::Overflow => "rational intermediate exceeds i64 representation",
            Self::Irrational => "SoftLight square root is irrational",
        })
    }
}

impl std::error::Error for Error {}

fn validate_float(value: f32) -> Result<(), Error> {
    if value.is_finite() && (0.0..=1.0).contains(&value) { Ok(()) } else { Err(Error::OutOfRange) }
}

fn validate_ratio(value: Ratio) -> Result<(), Error> {
    if (Ratio::ZERO..=Ratio::ONE).contains(&value) { Ok(()) } else { Err(Error::OutOfRange) }
}

fn screen(backdrop: f32, source: f32) -> f32 {
    backdrop + source - backdrop * source
}

fn hard_light(backdrop: f32, source: f32) -> f32 {
    if source <= 0.5 { backdrop * (2.0 * source) } else { screen(backdrop, 2.0 * source - 1.0) }
}

fn blend_channel(mode: Mode, backdrop: f32, source: f32) -> f32 {
    match mode {
        Mode::Normal => source,
        Mode::Multiply => backdrop * source,
        Mode::Screen => screen(backdrop, source),
        Mode::Overlay => hard_light(source, backdrop),
        Mode::Darken => backdrop.min(source),
        Mode::Lighten => backdrop.max(source),
        Mode::ColorDodge => {
            if backdrop == 0.0 {
                0.0
            } else if source == 1.0 {
                1.0
            } else {
                (backdrop / (1.0 - source)).min(1.0)
            }
        }
        Mode::ColorBurn => {
            if backdrop == 1.0 {
                1.0
            } else if source == 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - backdrop) / source).min(1.0)
            }
        }
        Mode::HardLight => hard_light(backdrop, source),
        Mode::SoftLight => {
            if source <= 0.5 {
                backdrop - (1.0 - 2.0 * source) * backdrop * (1.0 - backdrop)
            } else {
                let curve = if backdrop <= 0.25 { ((16.0 * backdrop - 12.0) * backdrop + 4.0) * backdrop } else { backdrop.sqrt() };
                backdrop + (2.0 * source - 1.0) * (curve - backdrop)
            }
        }
        Mode::Difference => (backdrop - source).abs(),
        Mode::Exclusion => backdrop + source - 2.0 * backdrop * source,
    }
}

/// Blend two f32 straight colour channels with f32 arithmetic.
///
/// Returns [`Error::OutOfRange`] for non-finite or out-of-range inputs.
pub fn blend_f32(mode: Mode, backdrop: f32, source: f32) -> Result<f32, Error> {
    validate_float(backdrop)?;
    validate_float(source)?;
    Ok(blend_channel(mode, backdrop, source))
}

/// Source-over for f32 channels, with normalized weights to avoid tiny-alpha underflow.
///
/// Transparent output is canonical black. No denominator floor is applied, even for subnormal
/// alpha. Inputs outside `[0, 1]` (including NaN and infinity) return [`Error::OutOfRange`].
pub fn composite_f32(mode: Mode, backdrop: Channel<f32>, source: Channel<f32>) -> Result<Channel<f32>, Error> {
    for value in [backdrop.color, backdrop.alpha, source.color, source.alpha] {
        validate_float(value)?;
    }
    if source.alpha == 0.0 {
        return Ok(if backdrop.alpha == 0.0 { Channel { color: 0.0, alpha: 0.0 } } else { backdrop });
    }
    if backdrop.alpha == 0.0 || (mode == Mode::Normal && source.alpha == 1.0) {
        return Ok(source);
    }
    let backdrop_color = backdrop.color;
    let source_color = source.color;
    let backdrop_alpha = backdrop.alpha;
    let source_alpha = source.alpha;
    let blended = blend_channel(mode, backdrop_color, source_color);
    let alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
    let source_weight = source_alpha / alpha;
    let backdrop_weight = (1.0 - source_alpha) * (backdrop_alpha / alpha);
    let color = (1.0 - backdrop_alpha) * source_weight * source_color + backdrop_alpha * source_weight * blended + backdrop_weight * backdrop_color;
    Ok(Channel { color: color.clamp(0.0, 1.0), alpha })
}

fn screen_ratio(backdrop: Ratio, source: Ratio) -> Result<Ratio, Error> {
    backdrop.checked_add(source)?.checked_sub(backdrop.checked_mul(source)?)
}

fn hard_light_ratio(backdrop: Ratio, source: Ratio) -> Result<Ratio, Error> {
    let doubled = source.checked_mul(Ratio::from_integer(2))?;
    if doubled <= Ratio::ONE { backdrop.checked_mul(doubled) } else { screen_ratio(backdrop, doubled.checked_sub(Ratio::ONE)?) }
}

fn soft_light_ratio(backdrop: Ratio, source: Ratio) -> Result<Ratio, Error> {
    let doubled = source.checked_mul(Ratio::from_integer(2))?;
    if doubled <= Ratio::ONE {
        backdrop.checked_sub(Ratio::ONE.checked_sub(doubled)?.checked_mul(backdrop)?.checked_mul(Ratio::ONE.checked_sub(backdrop)?)?)
    } else {
        let curve = if backdrop.checked_mul(Ratio::from_integer(4))? <= Ratio::ONE {
            Ratio::from_integer(16)
                .checked_mul(backdrop)?
                .checked_sub(Ratio::from_integer(12))?
                .checked_mul(backdrop)?
                .checked_add(Ratio::from_integer(4))?
                .checked_mul(backdrop)?
        } else {
            backdrop.sqrt_exact()?
        };
        backdrop.checked_add(doubled.checked_sub(Ratio::ONE)?.checked_mul(curve.checked_sub(backdrop)?)?)
    }
}

/// Exact blending on normalized bounded rationals.
///
/// SoftLight returns [`Error::Irrational`] when its active square-root branch has no rational
/// value. Any unrepresentable intermediate returns [`Error::Overflow`], never a rounded value.
pub fn blend_rational(mode: Mode, backdrop: Ratio, source: Ratio) -> Result<Ratio, Error> {
    validate_ratio(backdrop)?;
    validate_ratio(source)?;
    match mode {
        Mode::Normal => Ok(source),
        Mode::Multiply => backdrop.checked_mul(source),
        Mode::Screen => screen_ratio(backdrop, source),
        Mode::Overlay => hard_light_ratio(source, backdrop),
        Mode::Darken => Ok(backdrop.min(source)),
        Mode::Lighten => Ok(backdrop.max(source)),
        Mode::ColorDodge => {
            if backdrop == Ratio::ZERO {
                Ok(Ratio::ZERO)
            } else if source == Ratio::ONE {
                Ok(Ratio::ONE)
            } else {
                Ok(backdrop.checked_div(Ratio::ONE.checked_sub(source)?)?.min(Ratio::ONE))
            }
        }
        Mode::ColorBurn => {
            if backdrop == Ratio::ONE {
                Ok(Ratio::ONE)
            } else if source == Ratio::ZERO {
                Ok(Ratio::ZERO)
            } else {
                Ratio::ONE.checked_sub(Ratio::ONE.checked_sub(backdrop)?.checked_div(source)?.min(Ratio::ONE))
            }
        }
        Mode::HardLight => hard_light_ratio(backdrop, source),
        Mode::SoftLight => soft_light_ratio(backdrop, source),
        Mode::Difference => backdrop.max(source).checked_sub(backdrop.min(source)),
        Mode::Exclusion => backdrop.checked_add(source)?.checked_sub(Ratio::from_integer(2).checked_mul(backdrop)?.checked_mul(source)?),
    }
}

/// Exact source-over. Zero-alpha output is canonical black; invisible blend evaluations are skipped.
///
/// Errors are as for [`blend_rational`]. Transparent inputs can carry arbitrary hidden colour,
/// but identity laws preserve straight colour only when the surviving alpha is nonzero or its
/// hidden colour was already canonical zero.
pub fn composite_rational(mode: Mode, backdrop: Channel<Ratio>, source: Channel<Ratio>) -> Result<Channel<Ratio>, Error> {
    for value in [backdrop.color, backdrop.alpha, source.color, source.alpha] {
        validate_ratio(value)?;
    }
    if source.alpha == Ratio::ZERO {
        return Ok(if backdrop.alpha == Ratio::ZERO { Channel { color: Ratio::ZERO, alpha: Ratio::ZERO } } else { backdrop });
    }
    if backdrop.alpha == Ratio::ZERO || (mode == Mode::Normal && source.alpha == Ratio::ONE) {
        return Ok(source);
    }
    let blended = blend_rational(mode, backdrop.color, source.color)?;
    let source_only = Ratio::ONE.checked_sub(backdrop.alpha)?.checked_mul(source.alpha)?.checked_mul(source.color)?;
    let overlap = backdrop.alpha.checked_mul(source.alpha)?.checked_mul(blended)?;
    let backdrop_weight = Ratio::ONE.checked_sub(source.alpha)?.checked_mul(backdrop.alpha)?;
    let backdrop_only = backdrop_weight.checked_mul(backdrop.color)?;
    let alpha = source.alpha.checked_add(backdrop_weight)?;
    let color = source_only.checked_add(overlap)?.checked_add(backdrop_only)?.checked_div(alpha)?;
    Ok(Channel { color, alpha })
}
