use craft_blend::{Channel, Error, Mode, Ratio, blend_f32, blend_rational, composite_f32, composite_rational};

fn parse_ratio(text: &str) -> Result<Ratio, Box<dyn std::error::Error>> {
    let (numerator, denominator) = text.split_once('/').ok_or("missing fraction separator")?;
    Ok(Ratio::new(numerator.parse()?, denominator.parse()?)?)
}

fn parse_mode(text: &str) -> Result<Mode, Box<dyn std::error::Error>> {
    Ok(match text {
        "normal" => Mode::Normal,
        "multiply" => Mode::Multiply,
        "screen" => Mode::Screen,
        "overlay" => Mode::Overlay,
        "darken" => Mode::Darken,
        "lighten" => Mode::Lighten,
        "colorDodge" => Mode::ColorDodge,
        "colorBurn" => Mode::ColorBurn,
        "hardLight" => Mode::HardLight,
        "softLight" => Mode::SoftLight,
        "difference" => Mode::Difference,
        "exclusion" => Mode::Exclusion,
        _ => return Err(format!("unknown mode {text}").into()),
    })
}

fn assert_ulp(actual: f32, expected: Ratio, row: usize, field: &str) -> u32 {
    let reference = expected.to_f64() as f32;
    assert!(actual.is_finite() && (0.0..=1.0).contains(&actual), "row {row} {field}: {actual}");
    let ulps = actual.to_bits().abs_diff(reference.to_bits());
    assert!(ulps <= 2, "row {row} {field}: {actual} versus {reference} ({expected}), {ulps} ULP");
    ulps
}

#[test]
fn lean_generated_conformance() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    let mut maximum_ulp = 0;
    let mut mode_counts = [0_usize; 12];
    let table = include_str!("vectors/lean.csv");
    assert_eq!(table.lines().next(), Some("mode,cb,cs,ab,as,blend,premul,color,alpha"));
    for (row, line) in table.lines().enumerate().skip(1) {
        let columns: Vec<_> = line.split(',').collect();
        assert_eq!(columns.len(), 9, "row {row}");
        let mode = parse_mode(columns[0])?;
        let values: Vec<_> = columns[1..].iter().map(|text| parse_ratio(text)).collect::<Result<_, _>>()?;
        let backdrop = Channel { color: values[0], alpha: values[2] };
        let source = Channel { color: values[1], alpha: values[3] };
        let expected = Channel { color: values[6], alpha: values[7] };
        assert_eq!(blend_rational(mode, backdrop.color, source.color)?, values[4], "row {row} blend");
        let exact = composite_rational(mode, backdrop, source)?;
        assert_eq!(exact, expected, "row {row} composite");
        assert_eq!(exact.color.checked_mul(exact.alpha)?, values[5], "row {row} premultiplied");
        let backdrop_float = Channel { color: backdrop.color.to_f64() as f32, alpha: backdrop.alpha.to_f64() as f32 };
        let source_float = Channel { color: source.color.to_f64() as f32, alpha: source.alpha.to_f64() as f32 };
        let floating = composite_f32(mode, backdrop_float, source_float)?;
        maximum_ulp = maximum_ulp.max(assert_ulp(floating.color, expected.color, row, "color"));
        maximum_ulp = maximum_ulp.max(assert_ulp(floating.alpha, expected.alpha, row, "alpha"));
        maximum_ulp = maximum_ulp.max(assert_ulp(blend_f32(mode, backdrop_float.color, source_float.color)?, values[4], row, "blend"));
        mode_counts[mode as usize] += 1;
        count += 1;
    }
    assert_eq!(count, 4075);
    assert!(mode_counts.iter().all(|&mode_count| mode_count >= 200));
    println!("{count} Lean vectors: rational exact; maximum f32 error {maximum_ulp} ULP (limit 2)");
    Ok(())
}

#[test]
fn ticket_regression() -> Result<(), Error> {
    let half = Ratio::new(1, 2)?;
    let backdrop = Channel { color: half, alpha: half };
    let source = Channel { color: Ratio::new(4, 5)?, alpha: half };
    let output = composite_rational(Mode::Multiply, backdrop, source)?;
    let old_blend = blend_rational(Mode::Multiply, backdrop.color, source.color)?;
    let old = backdrop.color.checked_add(old_blend.checked_sub(backdrop.color)?.checked_mul(source.alpha.checked_div(output.alpha)?)?)?;
    assert_eq!(output, Channel { color: Ratio::new(17, 30)?, alpha: Ratio::new(3, 4)? });
    assert_eq!(old, Ratio::new(13, 30)?);
    assert_ne!(old, output.color);
    Ok(())
}

#[test]
fn soft_light_is_not_closed_over_rationals() -> Result<(), Error> {
    assert_eq!(blend_rational(Mode::SoftLight, Ratio::new(1, 2)?, Ratio::ONE), Err(Error::Irrational));
    assert_eq!(blend_rational(Mode::SoftLight, Ratio::new(9, 16)?, Ratio::ONE)?, Ratio::new(3, 4)?);
    assert_eq!(blend_rational(Mode::SoftLight, Ratio::new(1, 4)?, Ratio::ONE)?, Ratio::new(1, 2)?);
    assert_eq!(blend_f32(Mode::SoftLight, 0.5, 1.0)?, 0.5_f64.sqrt() as f32);
    let invisible = Channel { color: Ratio::new(1, 2)?, alpha: Ratio::ZERO };
    let source = Channel { color: Ratio::ONE, alpha: Ratio::ONE };
    assert_eq!(composite_rational(Mode::SoftLight, invisible, source)?, source);
    Ok(())
}

#[test]
fn transparent_canonicalization_and_identities() -> Result<(), Error> {
    let clear = Channel { color: Ratio::ZERO, alpha: Ratio::ZERO };
    let hidden = Channel { color: Ratio::ONE, alpha: Ratio::ZERO };
    let visible = Channel { color: Ratio::new(2, 3)?, alpha: Ratio::new(1, 7)? };
    assert_eq!(composite_rational(Mode::Multiply, hidden, hidden)?, clear);
    assert_eq!(composite_rational(Mode::SoftLight, visible, hidden)?, visible);
    assert_eq!(composite_rational(Mode::SoftLight, hidden, visible)?, visible);
    let opaque = Channel { color: visible.color, alpha: Ratio::ONE };
    assert_eq!(composite_rational(Mode::Normal, visible, opaque)?, opaque);
    let hidden_float = Channel { color: 1.0, alpha: 0.0 };
    assert_eq!(composite_f32(Mode::Multiply, hidden_float, hidden_float)?, Channel { color: 0.0, alpha: 0.0 });
    let visible_float = Channel { color: 0.8, alpha: 0.3 };
    assert_eq!(composite_f32(Mode::SoftLight, visible_float, hidden_float)?, visible_float);
    assert_eq!(composite_f32(Mode::SoftLight, hidden_float, visible_float)?, visible_float);
    let opaque_float = Channel { color: 0.8, alpha: 1.0 };
    for alpha in [0.0, f32::from_bits(1), 0.1, 0.3, 0.5, 0.9, 1.0] {
        assert_eq!(composite_f32(Mode::Normal, Channel { color: 0.7, alpha }, opaque_float)?, opaque_float);
    }
    Ok(())
}

#[test]
fn endpoints_and_tiny_alpha_have_no_floor() -> Result<(), Error> {
    assert_eq!(blend_f32(Mode::ColorDodge, 0.0, 1.0)?, 0.0);
    assert_eq!(blend_f32(Mode::ColorBurn, 1.0, 0.0)?, 1.0);
    for alpha in [f32::from_bits(1), f32::MIN_POSITIVE, 1e-8] {
        let backdrop = Channel { color: 0.5, alpha };
        let source = Channel { color: 0.75, alpha };
        let output = composite_f32(Mode::Normal, backdrop, source)?;
        assert!(output.alpha > 0.0);
        assert!((output.color - 0.625).abs() <= f32::EPSILON);
    }
    Ok(())
}

#[test]
fn invalid_inputs_are_errors() -> Result<(), Error> {
    let zero = Channel { color: 0.0, alpha: 0.0 };
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
        assert_eq!(blend_f32(Mode::Normal, invalid, 0.0), Err(Error::OutOfRange));
        assert_eq!(blend_f32(Mode::Normal, 0.0, invalid), Err(Error::OutOfRange));
        assert_eq!(composite_f32(Mode::Normal, zero, Channel { color: invalid, alpha: 0.0 }), Err(Error::OutOfRange));
        assert_eq!(composite_f32(Mode::Normal, Channel { color: 0.0, alpha: invalid }, zero), Err(Error::OutOfRange));
    }
    assert_eq!(blend_rational(Mode::Normal, Ratio::new(-1, 2)?, Ratio::ZERO), Err(Error::OutOfRange));
    assert_eq!(blend_rational(Mode::Normal, Ratio::ZERO, Ratio::new(3, 2)?), Err(Error::OutOfRange));
    Ok(())
}

#[test]
fn ratio_normalization_ordering_and_limits() -> Result<(), Error> {
    assert_eq!(Ratio::new(2, -4)?, Ratio::new(-1, 2)?);
    assert_eq!(Ratio::new(0, i64::MIN)?, Ratio::ZERO);
    assert_eq!(Ratio::new(i64::MIN, i64::MIN)?, Ratio::ONE);
    assert_eq!(Ratio::new(i64::MIN, -1), Err(Error::Overflow));
    assert_eq!(Ratio::new(1, i64::MIN), Err(Error::Overflow));
    assert_eq!(Ratio::new(1, 0), Err(Error::ZeroDenominator));
    assert_eq!(Ratio::ONE.checked_div(Ratio::ZERO), Err(Error::ZeroDenominator));
    let largest = Ratio::from_integer(i64::MAX);
    let smallest = Ratio::from_integer(i64::MIN);
    assert_eq!(largest.checked_add(largest), Err(Error::Overflow));
    assert_eq!(smallest.checked_sub(Ratio::ONE), Err(Error::Overflow));
    assert_eq!(largest.checked_mul(largest), Err(Error::Overflow));
    assert_eq!(largest.checked_div(largest)?, Ratio::ONE);
    assert_eq!(smallest.checked_sub(smallest)?, Ratio::ZERO);
    assert_eq!(Ratio::new(i64::MAX - 1, i64::MAX)?.checked_mul(Ratio::new(i64::MAX, i64::MAX - 1)?)?, Ratio::ONE);
    assert!(Ratio::new(i64::MAX - 1, i64::MAX)? < Ratio::ONE);
    assert_eq!(Ratio::new(2, 3)?.to_string(), "2/3");
    Ok(())
}
