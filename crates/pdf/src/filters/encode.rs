use super::{Error, Filter, FlateWrapper, Predictor, predictor};
use flate2::{
    Compression,
    write::{DeflateEncoder, ZlibEncoder},
};
use std::io::Write;
use weezl::{BitOrder, encode::Encoder};

/// Encoding options; predictor-bearing filters carry their own parameters.
#[derive(Clone, Copy, Debug)]
pub struct EncodeParams {
    /// Flate compression level, from 0 through 9.
    pub level: u32,
    /// Optional predictor for a plain Flate filter.
    pub predictor: Option<Predictor>,
}

impl Default for EncodeParams {
    fn default() -> Self {
        Self { level: 6, predictor: None }
    }
}

/// Encode a single filter. Unsupported filters and invalid parameters return an error.
pub fn encode(filter: Filter, data: &[u8], params: EncodeParams) -> Result<Vec<u8>, Error> {
    match filter {
        Filter::Flate | Filter::FlatePredictor(_) | Filter::FlateWithWrapper { .. } => {
            if params.level > 9 {
                return Err(Error::Corrupt);
            }
            let prediction = match filter {
                Filter::FlatePredictor(value) => Some(value),
                Filter::FlateWithWrapper { predictor, .. } => predictor,
                _ => params.predictor,
            };
            let bytes = if let Some(value) = prediction { predictor::encode(value, data)? } else { data.to_vec() };
            if matches!(filter, Filter::FlateWithWrapper { wrapper: FlateWrapper::Raw, .. }) {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(params.level));
                encoder.write_all(&bytes).map_err(|error| Error::Inflate(craft_archive::InflateError::Io(error)))?;
                return encoder.finish().map_err(|error| Error::Inflate(craft_archive::InflateError::Io(error)));
            }
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(params.level));
            encoder.write_all(&bytes).map_err(|error| Error::Inflate(craft_archive::InflateError::Io(error)))?;
            encoder.finish().map_err(|error| Error::Inflate(craft_archive::InflateError::Io(error)))
        }
        Filter::Lzw { early_change } | Filter::LzwPredictor { early_change, .. } => {
            let prediction = match filter {
                Filter::LzwPredictor { predictor, .. } => Some(predictor),
                _ => params.predictor,
            };
            let bytes = if let Some(value) = prediction { predictor::encode(value, data)? } else { data.to_vec() };
            let mut encoder = if early_change { Encoder::with_tiff_size_switch(BitOrder::Msb, 8) } else { Encoder::new(BitOrder::Msb, 8) };
            encoder.encode(&bytes).map_err(|_| Error::Corrupt)
        }
        Filter::AsciiHex => {
            let mut output = Vec::new();
            for (index, &byte) in data.iter().enumerate() {
                if index > 0 && index % 32 == 0 {
                    output.push(b'\n');
                }
                output.push(b"0123456789ABCDEF"[usize::from(byte >> 4)]);
                output.push(b"0123456789ABCDEF"[usize::from(byte & 15)]);
            }
            output.push(b'>');
            Ok(output)
        }
        Filter::Ascii85 => {
            let mut output = Vec::new();
            let mut column = 0;
            for chunk in data.chunks(4) {
                let mut word = [0; 4];
                word[..chunk.len()].copy_from_slice(chunk);
                let mut value = u32::from_be_bytes(word);
                let mut digits = [0; 5];
                let length = if value == 0 && chunk.len() == 4 {
                    digits[0] = b'z';
                    1
                } else {
                    for digit in digits.iter_mut().rev() {
                        *digit = (value % 85) as u8 + b'!';
                        value /= 85;
                    }
                    chunk.len() + 1
                };
                for byte in &digits[..length] {
                    if column == 75 {
                        output.push(b'\n');
                        column = 0;
                    }
                    output.push(*byte);
                    column += 1;
                }
            }
            output.extend_from_slice(b"~>");
            Ok(output)
        }
        Filter::RunLength => {
            let mut output = Vec::new();
            let mut index = 0;
            while index < data.len() {
                let mut run = 1;
                while index + run < data.len() && run < 128 && data[index + run] == data[index] {
                    run += 1;
                }
                if run >= 2 {
                    output.push((257 - run) as u8);
                    output.push(data[index]);
                    index += run;
                    continue;
                }
                let start = index;
                index += 1;
                while index < data.len() && index - start < 128 && !(index + 1 < data.len() && data[index] == data[index + 1]) {
                    index += 1;
                }
                output.push((index - start - 1) as u8);
                output.extend_from_slice(&data[start..index]);
            }
            output.push(128);
            Ok(output)
        }
        _ => Err(Error::Unsupported("encoder")),
    }
}

/// Encode in reverse decoding order; each stage has independent options.
pub fn encode_chain(filters: &[(Filter, EncodeParams)], data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut output = data.to_vec();
    for &(filter, params) in filters.iter().rev() {
        output = encode(filter, &output, params)?;
    }
    Ok(output)
}
