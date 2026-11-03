//! Periodic YvV states in the well-scaled basis `(y, sigma Δy, sigma² Δ²y)`.
//! A zero-state traversal supplies the forcing in `(I - A^length) state = forcing`;
//! the second traversal uses that steady state, independently for each direction.
//! Writing the DC term as `gain * (input - y)` avoids cancelling three feedback
//! coefficients near sigma 65,536. Constant lines therefore remain exactly constant.

use crate::{Cancel, Error, LINE_BATCH, Recursive, resize};

type Block<const CHANNELS: usize> = [[f64; LINE_BATCH]; CHANNELS];
type Matrix = [[f64; 3]; 3];

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    std::array::from_fn(|row| std::array::from_fn(|column| (0..3).map(|inner| left[row][inner] * right[inner][column]).sum()))
}

fn system(coefficients: &Recursive, mut length: usize) -> Matrix {
    let scale = f64::from(coefficients.sigma);
    let first = -coefficients.feedback[1] - 2.0 * coefficients.feedback[2];
    let second = coefficients.feedback[2];
    let mut power = [
        [1.0 - coefficients.gain, first / scale, second / scale.powi(2)],
        [-scale * coefficients.gain, first, second / scale],
        [-scale.powi(2) * coefficients.gain, scale * (first - 1.0), second],
    ];
    let mut result = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    while length != 0 {
        if length & 1 != 0 {
            result = multiply(result, power);
        }
        length >>= 1;
        power = multiply(power, power);
    }
    std::array::from_fn(|row| std::array::from_fn(|column| f64::from(row == column) - result[row][column]))
}

fn solve<const CHANNELS: usize>(mut matrix: Matrix, mut state: [Block<CHANNELS>; 3]) -> [Block<CHANNELS>; 3] {
    for column in 0..3 {
        let mut pivot = column;
        for row in column + 1..3 {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        matrix.swap(column, pivot);
        state.swap(column, pivot);
        let divisor = matrix[column][column];
        for entry in &mut matrix[column] {
            *entry /= divisor;
        }
        for values in &mut state[column] {
            for value in values {
                *value /= divisor;
            }
        }
        let pivot_row = matrix[column];
        let pivot_state = state[column];
        for row in 0..3 {
            if row == column {
                continue;
            }
            let scale = matrix[row][column];
            for (value, pivot) in matrix[row].iter_mut().zip(pivot_row) {
                *value -= scale * pivot;
            }
            for (values, pivots) in state[row].iter_mut().zip(pivot_state) {
                for (value, pivot) in values.iter_mut().zip(pivots) {
                    *value -= scale * pivot;
                }
            }
        }
    }
    state
}

fn advance<const CHANNELS: usize>(coefficients: &Recursive, input: Block<CHANNELS>, state: &mut [Block<CHANNELS>; 3]) -> Block<CHANNELS> {
    let scale = f64::from(coefficients.sigma);
    let first = (-coefficients.feedback[1] - 2.0 * coefficients.feedback[2]) / scale;
    let second = coefficients.feedback[2] / scale.powi(2);
    let delta: Block<CHANNELS> = std::array::from_fn(|channel| {
        std::array::from_fn(|lane| {
            coefficients.gain * (input[channel][lane] - state[0][channel][lane]) + first * state[1][channel][lane] + second * state[2][channel][lane]
        })
    });
    let value = std::array::from_fn(|channel| std::array::from_fn(|lane| state[0][channel][lane] + delta[channel][lane]));
    let difference = std::array::from_fn(|channel| std::array::from_fn(|lane| scale * delta[channel][lane]));
    let curvature = std::array::from_fn(|channel| std::array::from_fn(|lane| scale * (difference[channel][lane] - state[1][channel][lane])));
    *state = [value, difference, curvature];
    value
}

pub(crate) fn filter<const ROWS: bool, const CHANNELS: usize>(
    coefficients: &Recursive,
    lines: &mut [&mut [[f32; CHANNELS]]],
    forward: &mut Vec<Block<CHANNELS>>,
    cancel: Cancel<'_>,
) -> Result<(), Error> {
    let length = if ROWS { lines[0].len() } else { lines.len() };
    let width = if ROWS { lines.len() } else { lines[0].len() };
    if length == 1 {
        return cancel.check();
    }
    resize(forward, length, [[0.0; LINE_BATCH]; CHANNELS])?;
    let matrix = system(coefficients, length);
    for start in (0..width).step_by(LINE_BATCH) {
        cancel.check()?;
        let count = (width - start).min(LINE_BATCH);
        let read = |position: usize| {
            std::array::from_fn(|channel| {
                std::array::from_fn(|lane| {
                    if lane >= count {
                        0.0
                    } else if ROWS {
                        f64::from(lines[start + lane][position][channel])
                    } else {
                        f64::from(lines[position][start + lane][channel])
                    }
                })
            })
        };
        let baseline: Block<CHANNELS> = read(0);
        let centered = |position| {
            let input: Block<CHANNELS> = read(position);
            std::array::from_fn(|channel| std::array::from_fn(|lane| input[channel][lane] - baseline[channel][lane]))
        };
        let mut state = [[[0.0; LINE_BATCH]; CHANNELS]; 3];
        for position in 0..length {
            advance(coefficients, centered(position), &mut state);
        }
        state = solve(matrix, state);
        for (position, destination) in forward.iter_mut().enumerate() {
            *destination = advance(coefficients, centered(position), &mut state);
        }
        state = [[[0.0; LINE_BATCH]; CHANNELS]; 3];
        for input in forward.iter().rev() {
            advance(coefficients, *input, &mut state);
        }
        state = solve(matrix, state);
        for position in (0..length).rev() {
            let output = advance(coefficients, forward[position], &mut state);
            for lane in 0..count {
                let pixel = std::array::from_fn(|channel| (output[channel][lane] + baseline[channel][lane]) as f32);
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
