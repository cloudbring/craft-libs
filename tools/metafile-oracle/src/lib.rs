//! Test-only crate: see tests/metafile_oracle.rs and docs/verification/metafile-oracle.md.
//!
//! Shared helpers for the old-vs-new differential of the craft-metafile swaps (#92).
use craft_compare::{
    Image, ImageTolerance, Verdict,
    image::{ImageReport, compare_images},
};

/// A rendered raster: straight or premultiplied RGBA8, identical conventions on both sides.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Raster {
    pub fn image(&self) -> Option<Image> {
        Image::from_rgba8(self.width as usize, self.height as usize, &self.rgba).ok()
    }
}

/// Metrics of two rasters, or `None` when the shapes differ.
pub fn raster_report(a: &Raster, b: &Raster) -> Option<ImageReport> {
    if (a.width, a.height) != (b.width, b.height) {
        return None;
    }
    compare_images(&a.image()?, &b.image()?, 1.0 / 255.0).ok()
}

/// Strict raster judgement: byte-identical is `Identical`; anything else is judged against zero
/// tolerance on every metric, so the first non-zero error is the `Exceeds` metric.
pub fn judge_rasters(a: &Option<Raster>, b: &Option<Raster>) -> Verdict {
    match (a, b) {
        (None, None) => Verdict::Identical,
        (Some(_), None) | (None, Some(_)) => Verdict::Exceeds { metric: "raster_presence".into(), value: 1.0, limit: 0.0 },
        (Some(x), Some(y)) => {
            if x == y {
                return Verdict::Identical;
            }
            match raster_report(x, y) {
                None => Verdict::Exceeds { metric: "image_shape".into(), value: 1.0, limit: 0.0 },
                Some(r) => {
                    let strict = ImageTolerance { max_abs: Some(0.0), ..Default::default() };
                    match strict.verdict(&r) {
                        Verdict::Identical | Verdict::Within { .. } => Verdict::Exceeds { metric: "raster_bytes".into(), value: 1.0, limit: 0.0 },
                        v => v,
                    }
                }
            }
        }
    }
}
