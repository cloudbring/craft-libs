use crate::{Error, validate_sigma};

/// A symmetric normalized sampled Gaussian calibrated to discrete variance `sigma²`.
/// Support is `2 * ceil(3 * sigma) + 1`. Zero returns `[1]`. No algorithmic sigma cutoff;
/// unrepresentable support and allocation failure return errors rather than silently clamping.
pub fn gaussian_kernel(sigma: f32) -> Result<Vec<f32>, Error> {
    validate_sigma(sigma)?;
    if sigma == 0.0 {
        return Ok(vec![1.0]);
    }
    let radius = (3.0 * f64::from(sigma)).ceil();
    if radius > (isize::MAX as usize / (2 * size_of::<f64>()) - 1) as f64 {
        return Err(Error::SizeOverflow);
    }
    let half = radius as isize;
    let target = f64::from(sigma).powi(2);
    let sample = |width: f64, weights: &mut Vec<f64>| {
        weights.clear();
        weights.extend((-half..=half).map(|offset| (-(offset as f64).powi(2) / (2.0 * width * width)).exp()));
        let sum: f64 = weights.iter().sum();
        weights.iter_mut().for_each(|weight| *weight /= sum);
    };
    let mut weights = Vec::new();
    weights.try_reserve_exact(2 * half as usize + 1).map_err(|_| Error::Allocation)?;
    let (mut low, mut high) = (f64::from(sigma) * 0.01, f64::from(sigma) * 2.0 + 1.0);
    for _ in 0..48 {
        let middle = (low + high) * 0.5;
        sample(middle, &mut weights);
        let variance: f64 = weights.iter().enumerate().map(|(index, weight)| weight * (index as f64 - half as f64).powi(2)).sum();
        if variance < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    sample((low + high) * 0.5, &mut weights);
    let mut kernel = Vec::new();
    kernel.try_reserve_exact(weights.len()).map_err(|_| Error::Allocation)?;
    kernel.extend(weights.into_iter().map(|weight| weight as f32));
    Ok(kernel)
}

/// Three Kovesi odd box widths, including their quantization at subpixel sigmas.
/// This is a raw description, not a promise that a requested tolerance can be met.
/// [`crate::Gaussian::new`] checks the actual variance before accepting BoxApprox.
pub fn box_widths(sigma: f32) -> Result<[usize; 3], Error> {
    validate_sigma(sigma)?;
    let sigma = f64::from(sigma);
    let ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    if ideal > (isize::MAX as usize / 6) as f64 {
        return Err(Error::SizeOverflow);
    }
    let mut lower = ideal.floor() as usize;
    if lower.is_multiple_of(2) {
        lower = lower.saturating_sub(1);
    }
    lower = lower.max(1);
    let lower64 = lower as f64;
    let count = ((12.0 * sigma * sigma - 3.0 * lower64 * lower64 - 12.0 * lower64 - 9.0) / (-4.0 * lower64 - 4.0)).round();
    Ok(std::array::from_fn(|index| if (index as f64) < count { lower } else { lower + 2 }))
}
