/// Luma / colour-difference coefficients.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorStandard {
    Rec601,
    #[default]
    Rec709,
    Rec2020,
}

impl ColorStandard {
    pub const ALL: [ColorStandard; 3] = [ColorStandard::Rec601, ColorStandard::Rec709, ColorStandard::Rec2020];
    /// (Kr, Kb).
    pub fn coefficients(self) -> (f32, f32) {
        match self {
            ColorStandard::Rec601 => (0.299, 0.114),
            ColorStandard::Rec709 => (0.2126, 0.0722),
            ColorStandard::Rec2020 => (0.2627, 0.0593),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            ColorStandard::Rec601 => "rec601",
            ColorStandard::Rec709 => "rec709",
            ColorStandard::Rec2020 => "rec2020",
        }
    }
    pub fn from_name(s: &str) -> Option<ColorStandard> {
        match s.to_ascii_lowercase().replace([' ', '.', '-', '_'], "").as_str() {
            "rec601" | "601" | "bt601" => Some(ColorStandard::Rec601),
            "rec709" | "709" | "bt709" => Some(ColorStandard::Rec709),
            "rec2020" | "2020" | "bt2020" => Some(ColorStandard::Rec2020),
            _ => None,
        }
    }
    /// (Y, Cb, Cr) of a straight RGB colour.
    pub fn ycbcr(self, rgb: [f32; 3]) -> [f32; 3] {
        let (kr, kb) = self.coefficients();
        let y = kr * rgb[0] + (1.0 - kr - kb) * rgb[1] + kb * rgb[2];
        [y, (rgb[2] - y) / (2.0 * (1.0 - kb)), (rgb[0] - y) / (2.0 * (1.0 - kr))]
    }
}

/// Which scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    WaveformRgb,
    WaveformLuma,
    /// Luma (plane 0) and chroma amplitude `√(Cb² + Cr²)` (plane 2).
    WaveformYc,
    /// Cb across, Cr up.
    VectorscopeYuv,
    /// Hue around the circle, saturation outwards (HLS).
    VectorscopeHls,
    /// R, G, B histograms (bins across, counts up).
    Histogram,
    ParadeRgb,
    ParadeYuv,
}

impl ScopeKind {
    pub const ALL: [ScopeKind; 8] = [
        ScopeKind::WaveformRgb,
        ScopeKind::WaveformLuma,
        ScopeKind::WaveformYc,
        ScopeKind::VectorscopeYuv,
        ScopeKind::VectorscopeHls,
        ScopeKind::Histogram,
        ScopeKind::ParadeRgb,
        ScopeKind::ParadeYuv,
    ];
    pub fn name(self) -> &'static str {
        match self {
            ScopeKind::WaveformRgb => "waveformRgb",
            ScopeKind::WaveformLuma => "waveformLuma",
            ScopeKind::WaveformYc => "waveformYc",
            ScopeKind::VectorscopeYuv => "vectorscopeYuv",
            ScopeKind::VectorscopeHls => "vectorscopeHls",
            ScopeKind::Histogram => "histogram",
            ScopeKind::ParadeRgb => "paradeRgb",
            ScopeKind::ParadeYuv => "paradeYuv",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ScopeKind::WaveformRgb => "Waveform (RGB)",
            ScopeKind::WaveformLuma => "Waveform (Luma)",
            ScopeKind::WaveformYc => "Waveform (YC)",
            ScopeKind::VectorscopeYuv => "Vectorscope YUV",
            ScopeKind::VectorscopeHls => "Vectorscope HLS",
            ScopeKind::Histogram => "Histogram",
            ScopeKind::ParadeRgb => "Parade (RGB)",
            ScopeKind::ParadeYuv => "Parade (YUV)",
        }
    }
    pub fn from_name(s: &str) -> Option<ScopeKind> {
        let n = s.to_ascii_lowercase().replace([' ', '(', ')', '_', '-'], "");
        Self::ALL.iter().copied().find(|k| k.name().to_ascii_lowercase() == n || k.label().to_ascii_lowercase().replace([' ', '(', ')'], "") == n)
    }
    /// Vectorscopes are square.
    pub fn is_vectorscope(self) -> bool {
        matches!(self, ScopeKind::VectorscopeYuv | ScopeKind::VectorscopeHls)
    }
}

/// Scope options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopeOpts {
    pub standard: ColorStandard,
    /// Float scale (−0.25…1.25) instead of 8-bit (0…1).
    pub float: bool,
    /// Clamp the signal to 0…1 before plotting.
    pub clamp: bool,
    /// Plot size.
    pub width: u32,
    pub height: u32,
}

impl Default for ScopeOpts {
    fn default() -> Self {
        ScopeOpts { standard: ColorStandard::Rec709, float: false, clamp: true, width: 256, height: 256 }
    }
}

impl ScopeOpts {
    /// Signal range shown on the value axis.
    pub fn range(&self) -> (f32, f32) {
        if self.float { (-0.25, 1.25) } else { (0.0, 1.0) }
    }
    /// Plot row (0 = top) of a value, or `None` when off-scale.
    fn row(&self, v: f32) -> Option<usize> {
        let (lo, hi) = self.range();
        let f = (v - lo) / (hi - lo);
        if !(0.0..=1.0).contains(&f) {
            return None;
        }
        let r = ((1.0 - f) * (self.height - 1) as f32).round() as usize;
        Some(r.min(self.height as usize - 1))
    }
}

pub struct Scope { pub width: u32, pub height: u32, pub counts: [Vec<u32>; 3] }
pub fn compute(pixels: &[[f32; 3]], width: usize, kind: ScopeKind, options: &ScopeOpts) -> Scope {
    let size = options.width as usize * options.height as usize;
    let mut scope = Scope { width: options.width, height: options.height, counts: std::array::from_fn(|_| vec![0; size]) };
    for (index, rgb) in pixels.iter().enumerate() {
        let rgb = if options.clamp { rgb.map(|value| value.clamp(0.0, 1.0)) } else { *rgb };
        plot(&mut scope, kind, options, rgb, (index % width) as f32 / width as f32);
    }
    scope
}
#[inline]
fn bump(s: &mut Scope, c: usize, x: usize, y: usize) {
    let i = y * s.width as usize + x.min(s.width as usize - 1);
    s.counts[c][i] += 1;
}

/// Add one pixel (straight RGB, at horizontal position `fx` 0…1 in the frame).
fn plot(s: &mut Scope, kind: ScopeKind, o: &ScopeOpts, c: [f32; 3], fx: f32) {
    let w = s.width as usize;
    let col = |fx: f32, x0: usize, span: usize| x0 + ((fx * span as f32) as usize).min(span - 1);
    match kind {
        ScopeKind::WaveformRgb => {
            let x = col(fx, 0, w);
            for (ch, v) in c.iter().enumerate() {
                if let Some(y) = o.row(*v) {
                    bump(s, ch, x, y);
                }
            }
        }
        ScopeKind::WaveformLuma => {
            let x = col(fx, 0, w);
            let yv = o.standard.ycbcr(c)[0];
            if let Some(y) = o.row(yv) {
                for ch in 0..3 {
                    bump(s, ch, x, y);
                }
            }
        }
        ScopeKind::WaveformYc => {
            let x = col(fx, 0, w);
            let [yv, cb, cr] = o.standard.ycbcr(c);
            if let Some(y) = o.row(yv) {
                bump(s, 0, x, y);
                bump(s, 1, x, y);
            }
            if let Some(y) = o.row((cb * cb + cr * cr).sqrt()) {
                bump(s, 2, x, y);
            }
        }
        ScopeKind::ParadeRgb | ScopeKind::ParadeYuv => {
            let third = (w / 3).max(1);
            let vals = if kind == ScopeKind::ParadeRgb {
                c
            } else {
                let [y, cb, cr] = o.standard.ycbcr(c);
                // Colour differences centred on mid-scale.
                [y, cb + 0.5, cr + 0.5]
            };
            for (ch, v) in vals.iter().enumerate() {
                let x = col(fx, ch * third, third);
                if let Some(y) = o.row(*v) {
                    // RGB parade: tinted per channel; YUV: Y white, Cb blue, Cr red.
                    match (kind, ch) {
                        (ScopeKind::ParadeYuv, 0) => {
                            for p in 0..3 {
                                bump(s, p, x, y);
                            }
                        }
                        (ScopeKind::ParadeYuv, 1) => bump(s, 2, x, y),
                        (ScopeKind::ParadeYuv, _) => bump(s, 0, x, y),
                        _ => bump(s, ch, x, y),
                    }
                }
            }
        }
        ScopeKind::VectorscopeYuv => {
            let [_, cb, cr] = o.standard.ycbcr(c);
            if let Some((x, y)) = vec_cell(s, cb, cr) {
                for ch in 0..3 {
                    bump(s, ch, x, y);
                }
            }
        }
        ScopeKind::VectorscopeHls => {
            let (hue, _, sat) = hls(c);
            let a = hue.to_radians();
            if let Some((x, y)) = vec_cell(s, sat * 0.5 * a.cos(), sat * 0.5 * a.sin()) {
                for ch in 0..3 {
                    bump(s, ch, x, y);
                }
            }
        }
        ScopeKind::Histogram => {
            let (lo, hi) = o.range();
            for (ch, v) in c.iter().enumerate() {
                let f = (v - lo) / (hi - lo);
                if (0.0..=1.0).contains(&f) {
                    let x = ((f * (w - 1) as f32).round() as usize).min(w - 1);
                    // Histograms keep their counts in row 0; the bars are drawn by `histogram_bars`.
                    bump(s, ch, x, 0);
                }
            }
        }
    }
}

/// Vectorscope cell of (u, v) in −0.5…0.5 (the disc fills the plot; +v up).
fn vec_cell(s: &Scope, u: f32, v: f32) -> Option<(usize, usize)> {
    let (w, h) = (s.width as f32, s.height as f32);
    let x = ((u + 0.5) * (w - 1.0)).round();
    let y = ((0.5 - v) * (h - 1.0)).round();
    if x < 0.0 || y < 0.0 || x >= w || y >= h {
        return None;
    }
    Some((x as usize, y as usize))
}

/// (hue degrees, lightness, saturation) of straight RGB (HLS).
pub fn hls(c: [f32; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, l, 0.0);
    }
    let s = if l > 0.5 { d / (2.0 - max - min).max(1e-6) } else { d / (max + min).max(1e-6) };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, l, s.min(1.0))
}
