#[derive(Clone, Copy)]
pub enum WaveformType { Rgb, Luma, Yc, YcNoChroma }
#[derive(Clone, Copy)]
pub enum ParadeType { Rgb, Yuv, RgbWhite }
pub struct Signal { pub w: usize, pub h: usize, pub rgb: Vec<[f32; 3]> }
impl Signal { pub fn len(&self) -> usize { self.rgb.len() } }
pub const VECTOR_EXTENT: f32 = 0.6;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Matrix {
    Bt601,
    #[default]
    Bt709,
    Bt2020Ncl,
}

impl Matrix {
    /// (Kr, Kb) luma coefficients.
    pub fn kr_kb(self) -> (f32, f32) {
        match self {
            Matrix::Bt601 => (0.299, 0.114),
            Matrix::Bt709 => (0.2126, 0.0722),
            Matrix::Bt2020Ncl => (0.2627, 0.0593),
        }
    }
    /// From an ISO/IEC 23091-2 `matrix_coefficients` code.
    pub fn from_code(c: u8) -> Option<Matrix> {
        match c {
            1 => Some(Matrix::Bt709),
            5 | 6 => Some(Matrix::Bt601),
            9 | 10 => Some(Matrix::Bt2020Ncl),
            _ => None,
        }
    }
}

pub fn rgb_to_ycbcr(r: f32, g: f32, b: f32, m: Matrix) -> [f32; 3] {
    let (kr, kb) = m.kr_kb();
    let kg = 1.0 - kr - kb;
    let y = kr * r + kg * g + kb * b;
    [y, (b - y) / (2.0 * (1.0 - kb)), (r - y) / (2.0 * (1.0 - kr))]
}

/// How scopes are computed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub matrix: Matrix,
    /// Clamp Signal: values are clamped to 0..1 before they are plotted.
    pub clamp: bool,
    /// Waveform / parade levels.
    pub rows: usize,
    /// Vectorscope grid size (square; odd, so the centre is a cell).
    pub vector_size: usize,
}

impl Default for Params {
    fn default() -> Self {
        Params { matrix: Matrix::Bt709, clamp: true, rows: 256, vector_size: 255 }
    }
}

impl Params {
    /// The plotted value range of waveform rows.
    pub fn range(&self) -> (f32, f32) {
        if self.clamp { (0.0, 1.0) } else { (-0.1, 1.1) }
    }
}

/// A count grid: `data[row * cols + col]`, row 0 = the lowest level.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub name: &'static str,
    pub cols: usize,
    pub rows: usize,
    pub data: Vec<u32>,
}

impl Grid {
    pub fn new(name: &'static str, cols: usize, rows: usize) -> Grid {
        Grid { name, cols, rows, data: vec![0; cols * rows] }
    }
    pub fn at(&self, col: usize, row: usize) -> u32 {
        self.data[row * self.cols + col]
    }
    pub fn total(&self) -> u64 {
        self.data.iter().map(|&c| c as u64).sum()
    }
    pub fn max(&self) -> u32 {
        self.data.iter().copied().max().unwrap_or(0)
    }
    /// The rows hit in a column, lowest first.
    pub fn rows_in(&self, col: usize) -> Vec<usize> {
        (0..self.rows).filter(|&r| self.at(col, r) > 0).collect()
    }
}

/// Waveform or parade: one grid per trace, columns = the signal's width.
#[derive(Clone, Debug, PartialEq)]
pub struct Waveform {
    pub lo: f32,
    pub hi: f32,
    /// Samples per column (the signal's height).
    pub per_column: usize,
    pub traces: Vec<Grid>,
}

impl Waveform {
    pub fn trace(&self, name: &str) -> Option<&Grid> {
        self.traces.iter().find(|t| t.name == name)
    }
    /// The value at the centre of a row.
    pub fn level(&self, row: usize, rows: usize) -> f32 {
        self.lo + (self.hi - self.lo) * row as f32 / (rows.max(2) - 1) as f32
    }
}

/// Row of value `v` in `[lo, hi]` over `rows` levels (None outside, or NaN).
#[inline]
pub fn row_of(v: f32, lo: f32, hi: f32, rows: usize) -> Option<usize> {
    let t = (v - lo) / (hi - lo);
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    Some(((t * (rows - 1) as f32).round() as usize).min(rows - 1))
}

/// Up to 4 (trace, value) pairs per sample.
type Hits = ([(u8, f32); 4], usize);

fn build(s: &Signal, p: &Params, names: &[&'static str], sample: impl Fn([f32; 3]) -> Hits) -> Waveform {
    let (lo, hi) = p.range();
    let rows = p.rows.max(2);
    let mut traces: Vec<Grid> = names.iter().map(|n| Grid::new(n, s.w, rows)).collect();
    for y in 0..s.h {
        for x in 0..s.w {
            let mut c = s.rgb[y * s.w + x];
            if p.clamp {
                c = c.map(|v| v.clamp(0.0, 1.0));
            }
            let (hits, n) = sample(c);
            for &(t, v) in &hits[..n] {
                let v = if p.clamp { v.clamp(0.0, 1.0) } else { v };
                if let Some(r) = row_of(v, lo, hi, rows) {
                    let g = &mut traces[t as usize];
                    g.data[r * g.cols + x] += 1;
                }
            }
        }
    }
    Waveform { lo, hi, per_column: s.h, traces }
}

/// The waveform of a signal.
pub fn waveform(s: &Signal, kind: WaveformType, p: &Params) -> Waveform {
    let m = p.matrix;
    let z = (0u8, 0.0f32);
    match kind {
        WaveformType::Rgb => build(s, p, &["R", "G", "B"], |[r, g, b]| ([(0, r), (1, g), (2, b), z], 3)),
        WaveformType::Luma => build(s, p, &["Y"], |[r, g, b]| ([(0, rgb_to_ycbcr(r, g, b, m)[0]), z, z, z], 1)),
        WaveformType::YcNoChroma => build(s, p, &["Y"], |[r, g, b]| ([(0, rgb_to_ycbcr(r, g, b, m)[0]), z, z, z], 1)),
        WaveformType::Yc => build(s, p, &["Y", "C"], |[r, g, b]| {
            let [y, cb, cr] = rgb_to_ycbcr(r, g, b, m);
            let c = (cb * cb + cr * cr).sqrt();
            ([(0, y), (1, y + c), (1, y - c), z], if c > 1e-6 { 3 } else { 2 })
        }),
    }
}

/// The parade of a signal (the traces are drawn side by side).
pub fn parade(s: &Signal, kind: ParadeType, p: &Params) -> Waveform {
    let m = p.matrix;
    let z = (0u8, 0.0f32);
    match kind {
        ParadeType::Rgb => build(s, p, &["R", "G", "B"], |[r, g, b]| ([(0, r), (1, g), (2, b), z], 3)),
        ParadeType::RgbWhite => build(s, p, &["R", "G", "B", "Y"], |[r, g, b]| ([(0, r), (1, g), (2, b), (3, rgb_to_ycbcr(r, g, b, m)[0])], 4)),
        ParadeType::Yuv => build(s, p, &["Y", "Cb", "Cr"], |[r, g, b]| {
            let [y, cb, cr] = rgb_to_ycbcr(r, g, b, m);
            ([(0, y), (1, cb + 0.5), (2, cr + 0.5), z], 3)
        }),
    }
}

// ------------------------------------------------------------------------------------ histogram

/// 256-bin histograms of R', G', B' and Y'.
#[derive(Clone, Debug, PartialEq)]
pub struct Histogram {
    pub r: Vec<u32>,
    pub g: Vec<u32>,
    pub b: Vec<u32>,
    pub y: Vec<u32>,
    pub samples: u32,
    /// Samples below 0 / above 1 per channel (R, G, B, Y); with Clamp Signal they are also
    /// counted in the first / last bin.
    pub below: [u32; 4],
    pub above: [u32; 4],
}

pub const BINS: usize = 256;

/// Bin of a value (None when outside 0..1).
#[inline]
pub fn bin_of(v: f32) -> Option<usize> {
    if (0.0..=1.0 + 1e-6).contains(&v) { Some(((v * 255.0).round() as usize).min(BINS - 1)) } else { None }
}

pub fn histogram(s: &Signal, p: &Params) -> Histogram {
    let mut h = Histogram { r: vec![0; BINS], g: vec![0; BINS], b: vec![0; BINS], y: vec![0; BINS], samples: s.len() as u32, below: [0; 4], above: [0; 4] };
    for &[r, g, b] in &s.rgb {
        let y = rgb_to_ycbcr(r, g, b, p.matrix)[0];
        for (i, v) in [r, g, b, y].into_iter().enumerate() {
            let bins = match i {
                0 => &mut h.r,
                1 => &mut h.g,
                2 => &mut h.b,
                _ => &mut h.y,
            };
            match bin_of(v) {
                Some(k) => bins[k] += 1,
                None if v.is_nan() => {}
                None => {
                    let low = v < 0.0;
                    if low {
                        h.below[i] += 1;
                    } else {
                        h.above[i] += 1;
                    }
                    if p.clamp {
                        bins[if low { 0 } else { BINS - 1 }] += 1;
                    }
                }
            }
        }
    }
    h
}

// ------------------------------------------------------------------------------------ vectorscopes

/// A vectorscope: `data[y * size + x]`, x = Cb (right = +), y = Cr (row 0 = top = +Cr).
#[derive(Clone, Debug, PartialEq)]
pub struct Vectorscope {
    pub size: usize,
    pub extent: f32,
    pub data: Vec<u32>,
    pub samples: u32,
    /// Mean (Cb, Cr) of the plotted points.
    pub mean: [f32; 2],
}

/// Cell of a (Cb, Cr) point on a `size`² plane spanning ±`extent`.
#[inline]
pub fn cell_of(cb: f32, cr: f32, size: usize, extent: f32) -> Option<(usize, usize)> {
    let n = (size - 1) as f32;
    let fx = (cb / extent * 0.5 + 0.5) * n;
    let fy = (0.5 - cr / extent * 0.5) * n;
    if !(0.0..=n).contains(&fx) || !(0.0..=n).contains(&fy) {
        return None;
    }
    Some((fx.round() as usize, fy.round() as usize))
}

impl Vectorscope {
    fn new(size: usize) -> Self {
        Vectorscope { size, extent: VECTOR_EXTENT, data: vec![0; size * size], samples: 0, mean: [0.0; 2] }
    }
    pub fn at(&self, x: usize, y: usize) -> u32 {
        self.data[y * self.size + x]
    }
    pub fn cell(&self, cb: f32, cr: f32) -> Option<(usize, usize)> {
        cell_of(cb, cr, self.size, self.extent)
    }
    /// (Cb, Cr) at the centre of a cell.
    pub fn point(&self, x: usize, y: usize) -> [f32; 2] {
        let n = (self.size - 1) as f32;
        [(x as f32 / n - 0.5) * 2.0 * self.extent, (0.5 - y as f32 / n) * 2.0 * self.extent]
    }
    fn plot(&mut self, cb: f32, cr: f32, sum: &mut [f64; 2]) {
        if let Some((x, y)) = self.cell(cb, cr) {
            self.data[y * self.size + x] += 1;
            self.samples += 1;
            sum[0] += cb as f64;
            sum[1] += cr as f64;
        }
    }
    fn finish(mut self, sum: [f64; 2]) -> Self {
        if self.samples > 0 {
            self.mean = [(sum[0] / self.samples as f64) as f32, (sum[1] / self.samples as f64) as f32];
        }
        self
    }
}

/// The YUV vectorscope (Cb, Cr of each sample).
pub fn vectorscope_yuv(s: &Signal, p: &Params) -> Vectorscope {
    let mut v = Vectorscope::new(p.vector_size.max(2));
    let mut sum = [0f64; 2];
    for &c in &s.rgb {
        let [r, g, b] = if p.clamp { c.map(|x| x.clamp(0.0, 1.0)) } else { c };
        let [_, cb, cr] = rgb_to_ycbcr(r, g, b, p.matrix);
        v.plot(cb, cr, &mut sum);
    }
    v.finish(sum)
}
