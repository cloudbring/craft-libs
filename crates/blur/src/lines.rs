use crate::{Block, Edge, Error, LINE_BATCH, Recursive, Scratch, Separable, resize};

pub(crate) struct View<'a, 'b> {
    rows: &'a mut [&'b mut [f32]],
    channels: usize,
    start: usize,
    horizontal: bool,
}

impl<'a, 'b> View<'a, 'b> {
    pub(crate) fn rows(rows: &'a mut [&'b mut [f32]], channels: usize) -> Self {
        Self { rows, channels, start: 0, horizontal: true }
    }

    pub(crate) fn columns(rows: &'a mut [&'b mut [f32]], channels: usize, start: usize) -> Self {
        Self { rows, channels, start, horizontal: false }
    }

    fn length(&self) -> usize {
        if self.horizontal { self.rows[0].len() / self.channels } else { self.rows.len() }
    }

    fn count(&self) -> usize {
        if self.horizontal { self.rows.len() } else { (self.rows[0].len() / self.channels - self.start).min(LINE_BATCH) }
    }

    fn read(&self, position: usize) -> Block {
        let mut block = [[0.0; LINE_BATCH]; 4];
        for lane in 0..self.count() {
            let (row, column) = if self.horizontal { (lane, position) } else { (position, self.start + lane) };
            for (channel, values) in block.iter_mut().enumerate().take(self.channels) {
                values[lane] = f64::from(self.rows[row][column * self.channels + channel]);
            }
        }
        block
    }

    fn write(&mut self, position: usize, block: &Block) {
        for lane in 0..self.count() {
            let (row, column) = if self.horizontal { (lane, position) } else { (position, self.start + lane) };
            for (channel, values) in block.iter().enumerate().take(self.channels) {
                self.rows[row][column * self.channels + channel] = values[lane] as f32;
            }
        }
    }
}

pub(crate) fn filter(passes: &Separable, view: &mut View<'_, '_>, edge: Edge, scratch: &mut Scratch) -> Result<(), Error> {
    match view.channels {
        1 => filter_channels::<1>(passes, view, edge, scratch),
        2 => filter_channels::<2>(passes, view, edge, scratch),
        3 => filter_channels::<3>(passes, view, edge, scratch),
        _ => filter_channels::<4>(passes, view, edge, scratch),
    }
}

fn filter_channels<const CHANNELS: usize>(passes: &Separable, view: &mut View<'_, '_>, edge: Edge, scratch: &mut Scratch) -> Result<(), Error> {
    let length = view.length();
    let padding = match passes {
        Separable::Recursive(coefficients) if edge == Edge::Mirror => (32.0 * f64::from(coefficients.sigma)).ceil().max(64.0) as usize,
        _ => 0,
    };
    let extended = length.checked_add(padding.checked_mul(2).ok_or(Error::SizeOverflow)?).ok_or(Error::SizeOverflow)?;
    resize(&mut scratch.input, extended, [[0.0; LINE_BATCH]; 4])?;
    resize(&mut scratch.output, extended, [[0.0; LINE_BATCH]; 4])?;
    for position in 0..extended {
        let original = mapped(position as i128 - padding as i128, length, edge);
        scratch.input[position] = original.map_or([[0.0; LINE_BATCH]; 4], |index| view.read(index));
    }
    match passes {
        Separable::Kernel(kernel) => sampled::<CHANNELS>(&scratch.input, &mut scratch.output, kernel, edge),
        Separable::Recursive(coefficients) => {
            resize(&mut scratch.forward, extended, [[0.0; LINE_BATCH]; 4])?;
            recursive::<CHANNELS>(&scratch.input, &mut scratch.output, &mut scratch.forward, coefficients, edge);
        }
        Separable::Boxes(widths) => {
            resize(&mut scratch.forward, extended + 1, [[0.0; LINE_BATCH]; 4])?;
            for (index, width) in widths.iter().enumerate() {
                running_box::<CHANNELS>(&scratch.input, &mut scratch.output, &mut scratch.forward, width / 2, edge);
                if index != widths.len() - 1 {
                    std::mem::swap(&mut scratch.input, &mut scratch.output);
                }
            }
        }
    }
    for position in 0..length {
        view.write(position, &scratch.output[position + padding]);
    }
    Ok(())
}

pub(crate) fn mapped(position: i128, length: usize, edge: Edge) -> Option<usize> {
    if position >= 0 && position < length as i128 {
        return Some(position as usize);
    }
    match edge {
        Edge::Clamp => Some(if position < 0 { 0 } else { length - 1 }),
        Edge::Mirror => {
            let period = 2 * length as i128;
            let index = position.rem_euclid(period);
            Some(index.min(period - 1 - index) as usize)
        }
        Edge::Wrap => Some(position.rem_euclid(length as i128) as usize),
        Edge::Zero | Edge::Transparent => None,
    }
}

fn sampled<const CHANNELS: usize>(input: &[Block], output: &mut [Block], kernel: &[f32], edge: Edge) {
    let half = (kernel.len() / 2) as i128;
    for (position, destination) in output.iter_mut().enumerate() {
        *destination = [[0.0; LINE_BATCH]; 4];
        for (tap, weight) in kernel.iter().enumerate() {
            if let Some(index) = mapped(position as i128 + tap as i128 - half, input.len(), edge) {
                for channel in 0..CHANNELS {
                    for lane in 0..LINE_BATCH {
                        destination[channel][lane] += input[index][channel][lane] * f64::from(*weight);
                    }
                }
            }
        }
    }
}

fn recursive<const CHANNELS: usize>(input: &[Block], output: &mut [Block], forward: &mut [Block], coefficients: &Recursive, edge: Edge) {
    let zero = [[0.0; LINE_BATCH]; 4];
    let (left, right) = if edge == Edge::Zero || edge == Edge::Transparent { (zero, zero) } else { (input[0], input[input.len() - 1]) };
    let mut state = [zero; 3];
    for (position, destination) in forward.iter_mut().enumerate() {
        let mut value = zero;
        for channel in 0..CHANNELS {
            for lane in 0..LINE_BATCH {
                value[channel][lane] = coefficients.gain * (input[position][channel][lane] - left[channel][lane])
                    + coefficients.feedback[0] * state[0][channel][lane]
                    + coefficients.feedback[1] * state[1][channel][lane]
                    + coefficients.feedback[2] * state[2][channel][lane];
            }
        }
        state = [value, state[0], state[1]];
        *destination = value;
    }
    let mut future: [Block; 3] = coefficients.boundary.map(|row| {
        let mut block = zero;
        for channel in 0..CHANNELS {
            for lane in 0..LINE_BATCH {
                block[channel][lane] = row
                    .iter()
                    .zip(&state)
                    .map(|(coefficient, past)| coefficient * (past[channel][lane] + left[channel][lane] - right[channel][lane]))
                    .sum::<f64>();
            }
        }
        block
    });
    for position in (0..input.len()).rev() {
        if position != input.len() - 1 {
            let mut value = zero;
            for channel in 0..CHANNELS {
                for lane in 0..LINE_BATCH {
                    value[channel][lane] = coefficients.gain * (forward[position][channel][lane] + left[channel][lane] - right[channel][lane])
                        + coefficients.feedback[0] * future[0][channel][lane]
                        + coefficients.feedback[1] * future[1][channel][lane]
                        + coefficients.feedback[2] * future[2][channel][lane];
                }
            }
            future = [value, future[0], future[1]];
        }
        for channel in 0..CHANNELS {
            for lane in 0..LINE_BATCH {
                output[position][channel][lane] = future[0][channel][lane] + right[channel][lane];
            }
        }
    }
}

fn running_box<const CHANNELS: usize>(input: &[Block], output: &mut [Block], prefix: &mut [Block], radius: usize, edge: Edge) {
    prefix[0] = [[0.0; LINE_BATCH]; 4];
    for (position, block) in input.iter().enumerate() {
        for channel in 0..CHANNELS {
            for lane in 0..LINE_BATCH {
                prefix[position + 1][channel][lane] = prefix[position][channel][lane] + block[channel][lane];
            }
        }
    }
    let mut sum = [[0.0; LINE_BATCH]; 4];
    for (channel, values) in sum.iter_mut().enumerate().take(CHANNELS) {
        for (lane, value) in values.iter_mut().enumerate() {
            *value =
                extended_prefix(prefix, radius as i128 + 1, channel, lane, edge) - extended_prefix(prefix, -(radius as i128), channel, lane, edge);
        }
    }
    let divisor = (2 * radius + 1) as f64;
    for (position, destination) in output.iter_mut().enumerate() {
        for channel in 0..CHANNELS {
            for lane in 0..LINE_BATCH {
                destination[channel][lane] = f64::from((sum[channel][lane] / divisor) as f32);
            }
        }
        let added = mapped(position as i128 + radius as i128 + 1, input.len(), edge);
        let removed = mapped(position as i128 - radius as i128, input.len(), edge);
        for channel in 0..CHANNELS {
            for lane in 0..LINE_BATCH {
                sum[channel][lane] +=
                    added.map_or(0.0, |index| input[index][channel][lane]) - removed.map_or(0.0, |index| input[index][channel][lane]);
            }
        }
    }
}

fn extended_prefix(prefix: &[Block], position: i128, channel: usize, lane: usize, edge: Edge) -> f64 {
    let length = prefix.len() - 1;
    let total = prefix[length][channel][lane];
    if edge == Edge::Wrap {
        let cycles = position.div_euclid(length as i128);
        let remainder = position.rem_euclid(length as i128) as usize;
        return cycles as f64 * total + prefix[remainder][channel][lane];
    }
    if edge == Edge::Mirror {
        let period = 2 * length as i128;
        let cycles = position.div_euclid(period);
        let remainder = position.rem_euclid(period) as usize;
        let tail = if remainder <= length { prefix[remainder][channel][lane] } else { 2.0 * total - prefix[2 * length - remainder][channel][lane] };
        return cycles as f64 * (2.0 * total) + tail;
    }
    let mut value = prefix[position.clamp(0, length as i128) as usize][channel][lane];
    if edge == Edge::Clamp {
        if position < 0 {
            value += position as f64 * prefix[1][channel][lane];
        } else if position > length as i128 {
            value += (position - length as i128) as f64 * (total - prefix[length - 1][channel][lane]);
        }
    }
    value
}

#[cfg(test)]
mod tests;
