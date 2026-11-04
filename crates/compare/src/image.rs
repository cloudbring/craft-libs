//! Image comparison. Pixels are `f32`, nominally `0.0..=1.0` (so PSNR peak is 1.0); RGBA8 is
//! scaled by 1/255. Channel layouts: 1 (luma/plane), 2 (luma+alpha), 3 (sRGB), 4 (sRGB+alpha).
//! Alpha is compared by the error metrics but ignored by luma, SSIM and CIEDE2000.
//!
//! Error-metric shape follows vectorcraft `testkit::raster` and filmcraft's render tolerance;
//! SSIM is Wang et al. 2004 (Gaussian 11x11, sigma 1.5, K1 = 0.01, K2 = 0.03, L = 1, valid
//! region only); CIEDE2000 is Sharma, Wu and Dalal 2005 over sRGB -> XYZ (D65) -> Lab.

use crate::{CompareError, Verdict};

/// Interleaved image, `data.len() == width * height * channels`.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub data: Vec<f32>,
}

impl Image {
    pub fn new(width: usize, height: usize, channels: usize, data: Vec<f32>) -> Result<Self, CompareError> {
        if !(1..=4).contains(&channels) || data.len() != width * height * channels {
            return Err(CompareError::Shape(format!("{width}x{height}x{channels} needs {} samples, got {}", width * height * channels, data.len())));
        }
        Ok(Self { width, height, channels, data })
    }

    /// RGBA8 bytes scaled to `0.0..=1.0`.
    pub fn from_rgba8(width: usize, height: usize, rgba: &[u8]) -> Result<Self, CompareError> {
        Self::new(width, height, 4, rgba.iter().map(|b| f32::from(*b) / 255.0).collect())
    }

    /// A single `f32` plane.
    pub fn from_plane(width: usize, height: usize, plane: Vec<f32>) -> Result<Self, CompareError> {
        Self::new(width, height, 1, plane)
    }

    fn pixels(&self) -> usize {
        self.width * self.height
    }

    /// sRGB `[r, g, b]` of a pixel (gray replicated for 1/2 channels).
    fn rgb(&self, p: usize) -> [f64; 3] {
        let c = self.channels;
        let at = |i: usize| f64::from(self.data[p * c + i]);
        if c >= 3 { [at(0), at(1), at(2)] } else { [at(0); 3] }
    }

    /// Rec. 709 luma of the (non-linear) values.
    fn luma(&self) -> Vec<f64> {
        (0..self.pixels())
            .map(|p| {
                let [r, g, b] = self.rgb(p);
                0.2126 * r + 0.7152 * g + 0.0722 * b
            })
            .collect()
    }
}

fn same_shape(a: &Image, b: &Image) -> Result<(), CompareError> {
    if (a.width, a.height, a.channels) == (b.width, b.height, b.channels) {
        Ok(())
    } else {
        Err(CompareError::Shape(format!("{}x{}x{} vs {}x{}x{}", a.width, a.height, a.channels, b.width, b.height, b.channels)))
    }
}

fn abs_errors(a: &Image, b: &Image) -> Vec<f64> {
    a.data.iter().zip(&b.data).map(|(x, y)| (f64::from(*x) - f64::from(*y)).abs()).collect()
}

/// Largest per-sample absolute error.
pub fn max_abs_error(a: &Image, b: &Image) -> Result<f64, CompareError> {
    same_shape(a, b)?;
    Ok(abs_errors(a, b).into_iter().fold(0.0, f64::max))
}

/// Mean per-sample absolute error.
pub fn mean_abs_error(a: &Image, b: &Image) -> Result<f64, CompareError> {
    same_shape(a, b)?;
    let e = abs_errors(a, b);
    Ok(if e.is_empty() { 0.0 } else { e.iter().sum::<f64>() / e.len() as f64 })
}

/// 99th-percentile per-sample absolute error (nearest-rank).
pub fn p99_abs_error(a: &Image, b: &Image) -> Result<f64, CompareError> {
    same_shape(a, b)?;
    let mut e = abs_errors(a, b);
    if e.is_empty() {
        return Ok(0.0);
    }
    e.sort_by(f64::total_cmp);
    let rank = ((e.len() as f64) * 0.99).ceil() as usize;
    Ok(e[rank.clamp(1, e.len()) - 1])
}

/// PSNR in dB with peak 1.0; `+inf` when identical.
pub fn psnr(a: &Image, b: &Image) -> Result<f64, CompareError> {
    same_shape(a, b)?;
    let e = abs_errors(a, b);
    let mse = if e.is_empty() { 0.0 } else { e.iter().map(|x| x * x).sum::<f64>() / e.len() as f64 };
    Ok(if mse == 0.0 { f64::INFINITY } else { -10.0 * mse.log10() })
}

/// Fraction of pixels where any channel differs by more than `threshold`.
pub fn bad_pixel_fraction(a: &Image, b: &Image, threshold: f64) -> Result<f64, CompareError> {
    same_shape(a, b)?;
    if a.pixels() == 0 {
        return Ok(0.0);
    }
    let c = a.channels;
    let bad = (0..a.pixels()).filter(|p| (0..c).any(|i| (f64::from(a.data[p * c + i]) - f64::from(b.data[p * c + i])).abs() > threshold)).count();
    Ok(bad as f64 / a.pixels() as f64)
}

const WIN: usize = 11;
const SIGMA: f64 = 1.5;
const C1: f64 = 0.01 * 0.01;
const C2: f64 = 0.03 * 0.03;

fn gaussian_kernel() -> [f64; WIN] {
    let mut k = [0.0; WIN];
    let mut sum = 0.0;
    for (i, v) in k.iter_mut().enumerate() {
        let d = i as f64 - (WIN / 2) as f64;
        *v = (-(d * d) / (2.0 * SIGMA * SIGMA)).exp();
        sum += *v;
    }
    k.iter_mut().for_each(|v| *v /= sum);
    k
}

fn ssim_formula(mx: f64, my: f64, sxx: f64, syy: f64, sxy: f64) -> f64 {
    ((2.0 * mx * my + C1) * (2.0 * sxy + C2)) / ((mx * mx + my * my + C1) * (sxx + syy + C2))
}

/// Separable valid-region Gaussian filter of one plane.
fn gaussian_valid(q: &[f64], w: usize, h: usize, k: &[f64; WIN]) -> Vec<f64> {
    let (ow, oh) = (w - WIN + 1, h - WIN + 1);
    let mut tmp = vec![0.0; ow * h];
    for r in 0..h {
        for c in 0..ow {
            tmp[r * ow + c] = (0..WIN).map(|i| k[i] * q[r * w + c + i]).sum();
        }
    }
    let mut out = vec![0.0; ow * oh];
    for r in 0..oh {
        for c in 0..ow {
            out[r * ow + c] = (0..WIN).map(|i| k[i] * tmp[(r + i) * ow + c]).sum();
        }
    }
    out
}

/// Mean SSIM on luma. Images smaller than 11 pixels in either direction fall back to a single
/// global-statistics window. 1.0 when identical.
pub fn ssim(a: &Image, b: &Image) -> Result<f64, CompareError> {
    same_shape(a, b)?;
    let (w, h) = (a.width, a.height);
    if w == 0 || h == 0 {
        return Ok(1.0);
    }
    let (x, y) = (a.luma(), b.luma());
    if w < WIN || h < WIN {
        let n = x.len() as f64;
        let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
        let sxx = x.iter().map(|v| (v - mx) * (v - mx)).sum::<f64>() / n;
        let syy = y.iter().map(|v| (v - my) * (v - my)).sum::<f64>() / n;
        let sxy = x.iter().zip(&y).map(|(p, q)| (p - mx) * (q - my)).sum::<f64>() / n;
        return Ok(ssim_formula(mx, my, sxx, syy, sxy));
    }
    let k = gaussian_kernel();
    let xx: Vec<f64> = x.iter().map(|v| v * v).collect();
    let yy: Vec<f64> = y.iter().map(|v| v * v).collect();
    let xy: Vec<f64> = x.iter().zip(&y).map(|(p, q)| p * q).collect();
    let [fx, fy, fxx, fyy, fxy] = [&x, &y, &xx, &yy, &xy].map(|q| gaussian_valid(q, w, h, &k));
    let n = fx.len();
    let total: f64 = (0..n).map(|i| ssim_formula(fx[i], fy[i], fxx[i] - fx[i] * fx[i], fyy[i] - fy[i] * fy[i], fxy[i] - fx[i] * fy[i])).sum();
    Ok(total / n as f64)
}

/// CIE L*a*b* (D65) from sRGB in `0..=1`.
pub fn srgb_to_lab(rgb: [f64; 3]) -> [f64; 3] {
    let lin = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    let [r, g, b] = rgb.map(lin);
    let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b;
    let z = 0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x / 0.950_47), f(y), f(z / 1.088_83));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference between two Lab colours (Sharma et al. 2005, kL = kC = kH = 1).
pub fn delta_e_2000(lab1: [f64; 3], lab2: [f64; 3]) -> f64 {
    let [l1, a1, b1] = lab1;
    let [l2, a2, b2] = lab2;
    let (c1, c2) = (a1.hypot(b1), a2.hypot(b2));
    let cbar7 = ((c1 + c2) / 2.0).powi(7);
    let g = 0.5 * (1.0 - (cbar7 / (cbar7 + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = (a1p.hypot(b1), a2p.hypot(b2));
    let hue = |b: f64, a: f64| if b == 0.0 && a == 0.0 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
    let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dh_deg = if c1p * c2p == 0.0 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let dh = 2.0 * (c1p * c2p).sqrt() * (dh_deg.to_radians() / 2.0).sin();
    let lbar = (l1 + l2) / 2.0;
    let cbar = (c1p + c2p) / 2.0;
    let hbar = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * (hbar - 30.0).to_radians().cos() + 0.24 * (2.0 * hbar).to_radians().cos() + 0.32 * (3.0 * hbar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * hbar - 63.0).to_radians().cos();
    let dtheta = 30.0 * (-((hbar - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cbar.powi(7) / (cbar.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lbar - 50.0).powi(2) / (20.0 + (lbar - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cbar;
    let sh = 1.0 + 0.015 * cbar * t;
    let rt = -(2.0 * dtheta).to_radians().sin() * rc;
    let (xl, xc, xh) = (dl / sl, dc / sc, dh / sh);
    (xl * xl + xc * xc + xh * xh + rt * xc * xh).sqrt()
}

/// Mean and max CIEDE2000 over pixels (alpha ignored). Gray images are treated as neutral sRGB.
pub fn delta_e_stats(a: &Image, b: &Image) -> Result<(f64, f64), CompareError> {
    same_shape(a, b)?;
    let n = a.pixels();
    if n == 0 {
        return Ok((0.0, 0.0));
    }
    let (mut sum, mut max) = (0.0f64, 0.0f64);
    for p in 0..n {
        let d = delta_e_2000(srgb_to_lab(a.rgb(p)), srgb_to_lab(b.rgb(p)));
        sum += d;
        max = max.max(d);
    }
    Ok((sum / n as f64, max))
}

/// Every image metric in one value.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageReport {
    pub max_abs: f64,
    pub mean_abs: f64,
    pub p99_abs: f64,
    pub psnr_db: f64,
    pub ssim: f64,
    pub delta_e_mean: f64,
    pub delta_e_max: f64,
    /// Fraction of pixels over the bad-pixel threshold.
    pub bad_pixel_fraction: f64,
}

/// Compute an [`ImageReport`]; `bad_pixel_threshold` is in the `0..=1` sample scale.
pub fn compare_images(a: &Image, b: &Image, bad_pixel_threshold: f64) -> Result<ImageReport, CompareError> {
    let (delta_e_mean, delta_e_max) = delta_e_stats(a, b)?;
    Ok(ImageReport {
        max_abs: max_abs_error(a, b)?,
        mean_abs: mean_abs_error(a, b)?,
        p99_abs: p99_abs_error(a, b)?,
        psnr_db: psnr(a, b)?,
        ssim: ssim(a, b)?,
        delta_e_mean,
        delta_e_max,
        bad_pixel_fraction: bad_pixel_fraction(a, b, bad_pixel_threshold)?,
    })
}

/// Limits for an [`ImageReport`]; `None` skips a check.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageTolerance {
    pub max_abs: Option<f64>,
    pub mean_abs: Option<f64>,
    pub min_psnr_db: Option<f64>,
    pub min_ssim: Option<f64>,
    pub max_delta_e_mean: Option<f64>,
    pub max_delta_e_max: Option<f64>,
    pub max_bad_pixel_fraction: Option<f64>,
}

impl ImageTolerance {
    /// Judge a report: first exceeded limit wins, else `Within`, else `Identical`.
    pub fn verdict(&self, r: &ImageReport) -> Verdict {
        let upper = |name: &str, value: f64, limit: Option<f64>| limit.map(|l| Verdict::at_most(name, value, l));
        let checks = [
            upper("max_abs", r.max_abs, self.max_abs),
            upper("mean_abs", r.mean_abs, self.mean_abs),
            self.min_psnr_db.map(|l| Verdict::at_least("psnr_db", r.psnr_db, l, f64::INFINITY)),
            self.min_ssim.map(|l| Verdict::at_least("ssim", r.ssim, l, 1.0)),
            upper("delta_e_mean", r.delta_e_mean, self.max_delta_e_mean),
            upper("delta_e_max", r.delta_e_max, self.max_delta_e_max),
            upper("bad_pixel_fraction", r.bad_pixel_fraction, self.max_bad_pixel_fraction),
        ];
        Verdict::combine(checks.into_iter().flatten())
    }
}

/// Write an amplified per-pixel absolute difference (RGB, `gain` times the error, clamped) as PNG.
#[cfg(feature = "png")]
pub fn write_diff_png<W: std::io::Write>(a: &Image, b: &Image, gain: f32, out: W) -> Result<(), CompareError> {
    same_shape(a, b)?;
    let png_err = |e: &dyn std::fmt::Display| CompareError::Png(e.to_string());
    let c = a.channels;
    let mut buf = Vec::with_capacity(a.pixels() * 3);
    for p in 0..a.pixels() {
        let err = |i: usize| ((a.data[p * c + i] - b.data[p * c + i]).abs() * gain).clamp(0.0, 1.0);
        let rgb: [f32; 3] = if c >= 3 { [err(0), err(1), err(2)] } else { [err(0); 3] };
        buf.extend(rgb.iter().map(|v| (v * 255.0).round() as u8));
    }
    let width = u32::try_from(a.width).map_err(|e| png_err(&e))?;
    let height = u32::try_from(a.height).map_err(|e| png_err(&e))?;
    let mut enc = png::Encoder::new(out, width, height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| png_err(&e))?;
    w.write_image_data(&buf).map_err(|e| png_err(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize) -> Image {
        let data = (0..w * h * 4).map(|i| if i % 4 == 3 { 1.0 } else { ((i / 4) % w) as f32 / w as f32 }).collect();
        Image::new(w, h, 4, data).unwrap()
    }

    fn solid(w: usize, h: usize, rgb: [f32; 3]) -> Image {
        Image::new(w, h, 3, (0..w * h).flat_map(|_| rgb).collect()).unwrap()
    }

    #[test]
    fn identical_images() {
        let a = gradient(16, 16);
        let r = compare_images(&a, &a.clone(), 0.0).unwrap();
        assert_eq!(r.psnr_db, f64::INFINITY);
        assert_eq!((r.max_abs, r.mean_abs, r.p99_abs, r.delta_e_max, r.bad_pixel_fraction), (0.0, 0.0, 0.0, 0.0, 0.0));
        let tol = ImageTolerance { min_psnr_db: Some(40.0), min_ssim: Some(0.99), max_delta_e_max: Some(1.0), ..Default::default() };
        // SSIM of identical input can land a rounding error under 1.0; either verdict passes.
        assert!(tol.verdict(&r).passed());
        assert!((r.ssim - 1.0).abs() < 1e-12);
    }

    #[test]
    fn known_error_values() {
        let a = Image::from_plane(2, 2, vec![0.0, 0.0, 0.0, 0.0]).unwrap();
        let b = Image::from_plane(2, 2, vec![0.0, 0.0, 0.0, 0.5]).unwrap();
        assert_eq!(max_abs_error(&a, &b).unwrap(), 0.5);
        assert_eq!(mean_abs_error(&a, &b).unwrap(), 0.125);
        assert_eq!(p99_abs_error(&a, &b).unwrap(), 0.5);
        // mse = 0.25 / 4 = 0.0625 -> 12.0412 dB
        assert!((psnr(&a, &b).unwrap() - 12.0412).abs() < 1e-3);
        assert_eq!(bad_pixel_fraction(&a, &b, 0.1).unwrap(), 0.25);
        assert_eq!(bad_pixel_fraction(&a, &b, 0.5).unwrap(), 0.0);
    }

    #[test]
    fn rgba8_scales() {
        let a = Image::from_rgba8(1, 1, &[255, 0, 128, 255]).unwrap();
        assert_eq!(a.data[0], 1.0);
        assert!((a.data[2] - 128.0 / 255.0).abs() < 1e-6);
        assert!(Image::from_rgba8(2, 2, &[0; 4]).is_err());
    }

    #[test]
    fn shape_mismatch_is_error() {
        assert!(psnr(&gradient(4, 4), &gradient(5, 4)).is_err());
        assert!(ssim(&solid(2, 2, [0.0; 3]), &gradient(2, 2)).is_err());
    }

    #[test]
    fn ssim_drops_for_different_content() {
        let a = gradient(24, 24);
        let inv = Image::new(24, 24, 4, a.data.iter().enumerate().map(|(i, v)| if i % 4 == 3 { *v } else { 1.0 - v }).collect()).unwrap();
        let s = ssim(&a, &inv).unwrap();
        assert!(s < 0.5, "{s}");
        let mut slight = a.clone();
        slight.data[100] += 0.02;
        let s2 = ssim(&a, &slight).unwrap();
        assert!(s2 < 1.0 && s2 > 0.99, "{s2}");
        assert!((ssim(&solid(4, 4, [0.5; 3]), &solid(4, 4, [0.5; 3])).unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn lab_of_primaries() {
        let white = srgb_to_lab([1.0, 1.0, 1.0]);
        assert!((white[0] - 100.0).abs() < 0.01 && white[1].abs() < 0.01 && white[2].abs() < 0.01, "{white:?}");
        let black = srgb_to_lab([0.0; 3]);
        assert!(black[0].abs() < 1e-9);
        let red = srgb_to_lab([1.0, 0.0, 0.0]);
        assert!((red[0] - 53.24).abs() < 0.05 && (red[1] - 80.09).abs() < 0.1 && (red[2] - 67.20).abs() < 0.1, "{red:?}");
    }

    /// Sharma, Wu, Dalal 2005, supplementary test data (selected pairs).
    #[test]
    fn ciede2000_sharma_2005() {
        #[rustfmt::skip]
        let rows: [([f64; 3], [f64; 3], f64); 20] = [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 3.1571, -77.2803], [50.0, 0.0, -82.7485], 2.8615),
            ([50.0, 2.8361, -74.0200], [50.0, 0.0, -82.7485], 3.4412),
            ([50.0, -1.3802, -84.2814], [50.0, 0.0, -82.7485], 1.0000),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.49, -0.001], [50.0, -2.49, 0.0009], 7.1792),
            ([50.0, 2.49, -0.001], [50.0, -2.49, 0.0011], 7.2195),
            ([50.0, -0.001, 2.49], [50.0, 0.0009, -2.49], 4.8045),
            ([50.0, -0.001, 2.49], [50.0, 0.0011, -2.49], 4.7461),
            ([50.0, 2.5, 0.0], [50.0, 0.0, -2.5], 4.3065),
            ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
            ([50.0, 2.5, 0.0], [61.0, -5.0, 29.0], 22.8977),
            ([50.0, 2.5, 0.0], [56.0, -27.0, -3.0], 31.9030),
            ([50.0, 2.5, 0.0], [58.0, 24.0, 15.0], 19.4535),
            ([60.2574, -34.0099, 36.2677], [60.4626, -34.1751, 39.4387], 1.2644),
            ([63.0109, -31.0961, -5.8663], [62.8187, -29.7946, -4.0864], 1.2630),
            ([61.2901, 3.7196, -5.3901], [61.4292, 2.2480, -4.9620], 1.8731),
            ([22.7233, 20.0904, -46.6940], [23.0331, 14.9730, -42.5619], 2.0373),
            ([90.9257, -0.5406, -0.9208], [88.6381, -0.8985, -0.7239], 1.5381),
            ([2.0776, 0.0795, -1.1350], [0.9033, -0.0636, -0.5514], 0.9082),
        ];
        for (i, (l1, l2, want)) in rows.iter().enumerate() {
            let got = delta_e_2000(*l1, *l2);
            assert!((got - want).abs() < 1e-4, "row {i}: got {got}, want {want}");
            let rev = delta_e_2000(*l2, *l1);
            assert!((rev - got).abs() < 1e-9, "row {i} not symmetric");
        }
    }

    #[test]
    fn delta_e_between_images() {
        let (mean, max) = delta_e_stats(&solid(2, 2, [1.0, 1.0, 1.0]), &solid(2, 2, [0.0, 0.0, 0.0])).unwrap();
        assert!((mean - 100.0).abs() < 0.01 && (max - 100.0).abs() < 0.01);
    }

    #[test]
    fn tolerance_verdicts() {
        let a = solid(12, 12, [0.5; 3]);
        let b = solid(12, 12, [0.52; 3]);
        let r = compare_images(&a, &b, 0.01).unwrap();
        let loose = ImageTolerance { max_abs: Some(0.05), min_psnr_db: Some(30.0), ..Default::default() };
        assert!(matches!(loose.verdict(&r), Verdict::Within { .. }));
        let tight = ImageTolerance { max_abs: Some(0.001), ..Default::default() };
        assert!(matches!(tight.verdict(&r), Verdict::Exceeds { ref metric, .. } if metric == "max_abs"));
        assert_eq!(r.bad_pixel_fraction, 1.0);
    }

    #[cfg(feature = "png")]
    #[test]
    fn diff_png_signature() {
        let mut out = Vec::new();
        write_diff_png(&solid(3, 2, [0.0; 3]), &solid(3, 2, [0.5; 3]), 2.0, &mut out).unwrap();
        assert_eq!(&out[..8], b"\x89PNG\r\n\x1a\n");
    }
}
