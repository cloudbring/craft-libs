//! Clamp hot paths with channel-sized scratch, direct eight-chain recurrences, and sampled tiles.

use crate::{Cancel, Error, LINE_BATCH as LANES, Recursive, Scratch, Separable, Shape, resize};

type Block<const CHANNELS: usize> = [[f64; LANES]; CHANNELS];

#[derive(Default)]
pub(crate) struct Native<const CHANNELS: usize> {
    pub(crate) forward: Vec<Block<CHANNELS>>,
    pub(crate) padded: Vec<[f32; CHANNELS]>,
    pub(crate) values: Vec<[f64; CHANNELS]>,
    pub(crate) tile: Vec<[f32; CHANNELS]>,
}

pub(crate) fn apply(kernel: &[f32], pixels: &mut [f32], shape: Shape, scratch: &mut Scratch, cancel: Cancel<'_>) -> Result<(), Error> {
    match shape.channels() {
        1 => sampled_image(kernel, pixels, shape, &mut scratch.native1, cancel),
        2 => sampled_image(kernel, pixels, shape, &mut scratch.native2, cancel),
        3 => sampled_image(kernel, pixels, shape, &mut scratch.native3, cancel),
        _ => sampled_image(kernel, pixels, shape, &mut scratch.native4, cancel),
    }
}

pub(crate) fn rows(passes: &Separable, pixels: &mut [f32], shape: Shape, scratch: &mut Scratch, cancel: Cancel<'_>) -> Result<(), Error> {
    match shape.channels() {
        1 => rows_channels(passes, pixels, shape, &mut scratch.native1, cancel),
        2 => rows_channels(passes, pixels, shape, &mut scratch.native2, cancel),
        3 => rows_channels(passes, pixels, shape, &mut scratch.native3, cancel),
        _ => rows_channels(passes, pixels, shape, &mut scratch.native4, cancel),
    }
}

fn rows_channels<const CHANNELS: usize>(
    passes: &Separable,
    pixels: &mut [f32],
    shape: Shape,
    scratch: &mut Native<CHANNELS>,
    cancel: Cancel<'_>,
) -> Result<(), Error> {
    let pixels = pixels.as_chunks_mut::<CHANNELS>().0;
    let batch_pixels = shape.width().checked_mul(LANES).ok_or(Error::SizeOverflow)?;
    for batch in pixels.chunks_mut(batch_pixels) {
        match passes {
            Separable::Recursive(coefficients) => {
                let mut chunks = batch.chunks_mut(shape.width());
                let mut rows: [&mut [[f32; CHANNELS]]; LANES] = std::array::from_fn(|_| chunks.next().unwrap_or(&mut []));
                let count = rows.iter().take_while(|row| !row.is_empty()).count();
                coefficients.interleaved::<true, CHANNELS, _>(
                    &mut rows[..count],
                    &mut scratch.forward,
                    |pixel| pixel.map(f64::from),
                    |pixel| pixel.map(|value| value as f32),
                    cancel,
                )?;
            }
            Separable::Kernel(kernel) => {
                cancel.check()?;
                for row in batch.chunks_mut(shape.width()) {
                    sampled_row(kernel, row, scratch)?;
                }
            }
            Separable::Boxes(_) => return Err(Error::InvalidPasses),
        }
    }
    Ok(())
}

pub(crate) fn columns(
    coefficients: &Recursive,
    rows: &mut [&mut [f32]],
    channels: usize,
    scratch: &mut Scratch,
    cancel: Cancel<'_>,
) -> Result<(), Error> {
    match channels {
        1 => columns_channels(coefficients, rows, &mut scratch.native1, cancel),
        2 => columns_channels(coefficients, rows, &mut scratch.native2, cancel),
        3 => columns_channels(coefficients, rows, &mut scratch.native3, cancel),
        _ => columns_channels(coefficients, rows, &mut scratch.native4, cancel),
    }
}

fn columns_channels<const CHANNELS: usize>(
    coefficients: &Recursive,
    rows: &mut [&mut [f32]],
    scratch: &mut Native<CHANNELS>,
    cancel: Cancel<'_>,
) -> Result<(), Error> {
    let mut lines = Vec::new();
    lines.try_reserve_exact(rows.len()).map_err(|_| Error::Allocation)?;
    lines.extend(rows.iter_mut().map(|row| row.as_chunks_mut::<CHANNELS>().0));
    coefficients.interleaved::<false, CHANNELS, _>(
        &mut lines,
        &mut scratch.forward,
        |pixel| pixel.map(f64::from),
        |pixel| pixel.map(|value| value as f32),
        cancel,
    )
}

impl Recursive {
    /// Arithmetic from lightcraft 39aa4d25, crates/raster/src/blur/interleaved.rs::Gaussian::recursive.
    /// Only allocation failure handling and per-eight-line cancellation are added to the kernel.
    fn interleaved<const ROWS: bool, const CHANNELS: usize, T: Copy>(
        &self,
        lines: &mut [&mut [T]],
        forward: &mut Vec<Block<CHANNELS>>,
        widen: impl Fn(T) -> [f64; CHANNELS],
        narrow: impl Fn([f64; CHANNELS]) -> T,
        cancel: Cancel<'_>,
    ) -> Result<(), Error> {
        let length = if ROWS { lines[0].len() } else { lines.len() };
        let width = if ROWS { lines.len() } else { lines[0].len() };
        resize(forward, length, [[0.0; LANES]; CHANNELS])?;
        for start in (0..width).step_by(LANES) {
            cancel.check()?;
            let count = (width - start).min(LANES);
            let read = |position: usize| {
                let mut block = [[0.0; LANES]; CHANNELS];
                for lane in 0..count {
                    let pixel = if ROWS { lines[start + lane][position] } else { lines[position][start + lane] };
                    let channels = widen(pixel);
                    for channel in 0..CHANNELS {
                        block[channel][lane] = channels[channel];
                    }
                }
                block
            };
            let left = read(0);
            let right = read(length - 1);
            let mut state = [[[0.0; LANES]; CHANNELS]; 3];
            for (position, output) in forward.iter_mut().enumerate() {
                let input = read(position);
                let value = std::array::from_fn(|channel| {
                    std::array::from_fn(|lane| {
                        self.gain * (input[channel][lane] - left[channel][lane])
                            + self.feedback[0] * state[0][channel][lane]
                            + self.feedback[1] * state[1][channel][lane]
                            + self.feedback[2] * state[2][channel][lane]
                    })
                });
                state = [value, state[0], state[1]];
                *output = value;
            }
            let mut future: [Block<CHANNELS>; 3] = self.boundary.map(|row| {
                std::array::from_fn(|channel| {
                    std::array::from_fn(|lane| {
                        row.iter()
                            .zip(&state)
                            .map(|(coefficient, past)| coefficient * (past[channel][lane] + left[channel][lane] - right[channel][lane]))
                            .sum::<f64>()
                    })
                })
            });
            for position in (0..length).rev() {
                if position != length - 1 {
                    let value = std::array::from_fn(|channel| {
                        std::array::from_fn(|lane| {
                            self.gain * (forward[position][channel][lane] + left[channel][lane] - right[channel][lane])
                                + self.feedback[0] * future[0][channel][lane]
                                + self.feedback[1] * future[1][channel][lane]
                                + self.feedback[2] * future[2][channel][lane]
                        })
                    });
                    future = [value, future[0], future[1]];
                }
                for lane in 0..count {
                    let pixel = narrow(std::array::from_fn(|channel| future[0][channel][lane] + right[channel][lane]));
                    if ROWS {
                        lines[start + lane][position] = pixel;
                    } else {
                        lines[position][start + lane] = pixel;
                    }
                }
            }
        }
        Ok(())
    }
}

fn sampled_row<const CHANNELS: usize>(kernel: &[f32], pixels: &mut [[f32; CHANNELS]], scratch: &mut Native<CHANNELS>) -> Result<(), Error> {
    let half = kernel.len() / 2;
    let length = pixels.len().checked_add(2 * half).ok_or(Error::SizeOverflow)?;
    resize(&mut scratch.padded, length, [0.0; CHANNELS])?;
    let padded = &mut scratch.padded;
    padded[..half].fill(pixels[0]);
    padded[half..half + pixels.len()].copy_from_slice(pixels);
    padded[half + pixels.len()..].fill(pixels[pixels.len() - 1]);
    if let Ok(weights) = <&[f32; 7]>::try_from(kernel) {
        for (window, pixel) in padded.array_windows::<7>().zip(pixels) {
            let mut value = [0.0; CHANNELS];
            for tap in 0..7 {
                let input = window[tap].map(f64::from);
                for channel in 0..CHANNELS {
                    value[channel] += input[channel] * f64::from(weights[tap]);
                }
            }
            *pixel = value.map(|value| value as f32);
        }
        return Ok(());
    }
    let width = pixels.len();
    resize(&mut scratch.values, width, [0.0; CHANNELS])?;
    sampled_accumulate(kernel, pixels, &mut scratch.values, |tap| &padded[tap..tap + width]);
    Ok(())
}

fn sampled_accumulate<'a, const CHANNELS: usize>(
    kernel: &[f32],
    pixels: &mut [[f32; CHANNELS]],
    values: &mut [[f64; CHANNELS]],
    input: impl Fn(usize) -> &'a [[f32; CHANNELS]],
) {
    for (tap, &weight) in kernel.iter().enumerate() {
        if tap == 0 {
            for (value, &pixel) in values.iter_mut().zip(input(tap)) {
                *value = pixel.map(|channel| 0.0 + f64::from(channel) * f64::from(weight));
            }
        } else {
            for (value, &pixel) in values.iter_mut().zip(input(tap)) {
                for channel in 0..CHANNELS {
                    value[channel] += f64::from(pixel[channel]) * f64::from(weight);
                }
            }
        }
    }
    for (pixel, &value) in pixels.iter_mut().zip(values.iter()) {
        *pixel = value.map(|value| value as f32);
    }
}

/// In-place vertical tiles retain both halos from the unmodified horizontal result. Keeping
/// the trailing `2 * half` rows before advancing preserves the next tile's original top halo.
fn sampled_image<const CHANNELS: usize>(
    kernel: &[f32],
    pixels: &mut [f32],
    shape: Shape,
    scratch: &mut Native<CHANNELS>,
    cancel: Cancel<'_>,
) -> Result<(), Error> {
    let batch_pixels = shape.width().checked_mul(LANES).ok_or(Error::SizeOverflow)?;
    for batch in pixels.as_chunks_mut::<CHANNELS>().0.chunks_mut(batch_pixels) {
        cancel.check()?;
        for row in batch.chunks_mut(shape.width()) {
            sampled_row(kernel, row, scratch)?;
        }
    }
    const ROWS: usize = 64;
    let pixels = pixels.as_chunks_mut::<CHANNELS>().0;
    let width = shape.width();
    let half = kernel.len() / 2;
    let height = shape.height();
    let tile_rows = ROWS.checked_add(2 * half).ok_or(Error::SizeOverflow)?;
    resize(&mut scratch.tile, tile_rows.checked_mul(width).ok_or(Error::SizeOverflow)?, [0.0; CHANNELS])?;
    resize(&mut scratch.values, width, [0.0; CHANNELS])?;
    for start in (0..height).step_by(ROWS) {
        let count = (height - start).min(ROWS);
        let retained = if start == 0 { 0 } else { 2 * half };
        if retained != 0 {
            scratch.tile.copy_within(ROWS * width..(ROWS + retained) * width, 0);
        }
        for offset in retained..count + 2 * half {
            let source = (start + offset).saturating_sub(half).min(height - 1);
            scratch.tile[offset * width..(offset + 1) * width].copy_from_slice(&pixels[source * width..(source + 1) * width]);
        }
        for offset in 0..count {
            if offset.is_multiple_of(LANES) {
                cancel.check()?;
            }
            let row = &mut pixels[(start + offset) * width..(start + offset + 1) * width];
            sampled_accumulate(kernel, row, &mut scratch.values, |tap| &scratch.tile[(offset + tap) * width..(offset + tap + 1) * width]);
        }
    }
    Ok(())
}
