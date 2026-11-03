//! Bounded PDF stream decoding primitives.
//!
//! This module is independent of the reader and rasterizer features.

use craft_archive::{BoundedInflate, InflateError, Wrapper};
use weezl::{BitOrder, LzwStatus, decode::Decoder};

mod predictor;
pub use predictor::Predictor;

/// Per-stage and cumulative resource ceilings.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_output: u64,
    pub max_input: u64,
    pub max_ratio: u64,
    pub max_depth: usize,
}

/// How incomplete streams are handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Strict,
    Tolerant,
}

/// A supported stream decoding stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Flate,
    Lzw { early_change: bool },
    FlatePredictor(Predictor),
    LzwPredictor { early_change: bool, predictor: Predictor },
}

/// Bytes consumed and emitted by a stage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accounting {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub incomplete: bool,
}

/// A decoding failure.
#[derive(Debug)]
pub enum Error {
    InputLimit,
    OutputLimit,
    RatioLimit,
    DepthLimit,
    Truncated,
    Corrupt,
    Allocation,
    Inflate(InflateError),
}

/// Result of decoding all stages in the supplied order.
#[derive(Debug)]
pub struct Decoded {
    pub data: Vec<u8>,
    pub stages: Vec<Accounting>,
}

/// A chain of bounded stream decoders.
#[derive(Clone, Debug)]
pub struct FilterChain {
    pub filters: Vec<Filter>,
    pub limits: Limits,
    pub mode: Mode,
}

impl FilterChain {
    pub fn decode(&self, input: &[u8]) -> Result<Decoded, Error> {
        if self.filters.len() > self.limits.max_depth {
            return Err(Error::DepthLimit);
        }
        if input.len() as u64 > self.limits.max_input {
            return Err(Error::InputLimit);
        }
        let mut current = Vec::new();
        current.try_reserve_exact(input.len()).map_err(|_| Error::Allocation)?;
        current.extend_from_slice(input);
        let mut stages = Vec::new();
        stages.try_reserve_exact(self.filters.len()).map_err(|_| Error::Allocation)?;
        let mut total = 0u64;
        for filter in &self.filters {
            if current.len() as u64 > self.limits.max_input {
                return Err(Error::InputLimit);
            }
            let remaining = self.limits.max_output.saturating_sub(total);
            let (mut output, mut accounting, predictor) = match *filter {
                Filter::Flate => {
                    let (output, accounting) = self.flate(&current, remaining)?;
                    (output, accounting, None)
                }
                Filter::Lzw { early_change } => {
                    let (output, accounting) = self.lzw(&current, remaining, early_change)?;
                    (output, accounting, None)
                }
                Filter::FlatePredictor(predictor) => {
                    let (output, accounting) = self.flate(&current, remaining)?;
                    (output, accounting, Some(predictor))
                }
                Filter::LzwPredictor { early_change, predictor } => {
                    let (output, accounting) = self.lzw(&current, remaining, early_change)?;
                    (output, accounting, Some(predictor))
                }
            };
            if let Some(predictor) = predictor {
                let (decoded, incomplete) = predictor::decode(predictor, output, self.mode)?;
                output = decoded;
                accounting.incomplete |= incomplete;
                accounting.bytes_out = output.len() as u64;
            }
            total += accounting.bytes_out;
            stages.push(accounting);
            current = output;
        }
        if self.filters.is_empty() && current.len() as u64 > self.limits.max_output {
            return Err(Error::OutputLimit);
        }
        Ok(Decoded { data: current, stages })
    }

    fn flate(&self, input: &[u8], max_output: u64) -> Result<(Vec<u8>, Accounting), Error> {
        match self.inflate(input, max_output, Wrapper::Zlib) {
            Err(Error::Corrupt | Error::Truncated) if self.mode == Mode::Tolerant => self.inflate(input, max_output, Wrapper::Raw),
            result => result,
        }
    }

    fn inflate(&self, input: &[u8], max_output: u64, wrapper: Wrapper) -> Result<(Vec<u8>, Accounting), Error> {
        let limits = craft_archive::Limits { max_output, max_input: self.limits.max_input, max_ratio: self.limits.max_ratio, wrapper };
        let mut decoder = BoundedInflate::new(input, limits).map_err(InflateError::Io).map_err(Error::Inflate)?;
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut incomplete = false;
        loop {
            let count = match decoder.read_chunk(&mut buffer) {
                Ok(count) => count,
                Err(InflateError::Truncated) if self.mode == Mode::Tolerant => {
                    incomplete = true;
                    break;
                }
                Err(InflateError::Truncated) => return Err(Error::Truncated),
                Err(InflateError::Corrupt) => return Err(Error::Corrupt),
                Err(InflateError::InputLimit { .. }) => return Err(Error::InputLimit),
                Err(InflateError::OutputLimit { .. }) => return Err(Error::OutputLimit),
                Err(InflateError::RatioLimit) => return Err(Error::RatioLimit),
                Err(error) => return Err(Error::Inflate(error)),
            };
            if count == 0 {
                break;
            }
            output.try_reserve_exact(count).map_err(|_| Error::Allocation)?;
            output.extend_from_slice(&buffer[..count]);
        }
        let accounting = decoder.accounting();
        Ok((output, Accounting { bytes_in: accounting.bytes_in, bytes_out: accounting.bytes_out, incomplete }))
    }

    fn lzw(&self, input: &[u8], max_output: u64, early_change: bool) -> Result<(Vec<u8>, Accounting), Error> {
        let mut decoder = if early_change { Decoder::with_tiff_size_switch(BitOrder::Msb, 8) } else { Decoder::new(BitOrder::Msb, 8) };
        let mut output = Vec::new();
        let mut consumed = 0usize;
        let mut buffer = [0u8; 8192];
        loop {
            let result = decoder.decode_bytes(&input[consumed..], &mut buffer);
            consumed += result.consumed_in;
            let next = (output.len() as u64).saturating_add(result.consumed_out as u64);
            if next > max_output {
                return Err(Error::OutputLimit);
            }
            if next > (consumed as u64).saturating_mul(self.limits.max_ratio) {
                return Err(Error::RatioLimit);
            }
            output.try_reserve_exact(result.consumed_out).map_err(|_| Error::Allocation)?;
            output.extend_from_slice(&buffer[..result.consumed_out]);
            let incomplete = match result.status {
                Err(_) => return Err(Error::Corrupt),
                Ok(LzwStatus::Done) => false,
                Ok(LzwStatus::NoProgress) if self.mode == Mode::Tolerant => true,
                Ok(LzwStatus::NoProgress) => return Err(Error::Truncated),
                Ok(LzwStatus::Ok) => continue,
            };
            return Ok((output, Accounting { bytes_in: consumed as u64, bytes_out: next, incomplete }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use weezl::encode::Encoder;

    fn chain(filter: Filter, output: u64) -> FilterChain {
        FilterChain {
            filters: vec![filter],
            limits: Limits { max_input: 4096, max_output: output, max_ratio: 100, max_depth: 1 },
            mode: Mode::Strict,
        }
    }

    #[test]
    fn lzw_early_change_and_output_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let input: Vec<u8> = (0..4096).map(|index| (index % 251) as u8).collect();
        for early_change in [false, true] {
            let mut encoder = if early_change { Encoder::with_tiff_size_switch(BitOrder::Msb, 8) } else { Encoder::new(BitOrder::Msb, 8) };
            let encoded = encoder.encode(&input)?;
            let filter = Filter::Lzw { early_change };
            let decoded = chain(filter, input.len() as u64).decode(&encoded).map_err(|error| format!("{error:?}"))?;
            assert_eq!(decoded.data, input);
            assert!(matches!(chain(filter, input.len() as u64 - 1).decode(&encoded), Err(Error::OutputLimit)));
        }
        Ok(())
    }

    #[test]
    fn lzw_truncation_is_reported() -> Result<(), Box<dyn std::error::Error>> {
        let encoded = Encoder::new(BitOrder::Msb, 8).encode(b"testing incomplete lzw")?;
        let truncated = &encoded[..encoded.len() - 1];
        let mut chain = chain(Filter::Lzw { early_change: false }, 100);
        assert!(matches!(chain.decode(truncated), Err(Error::Truncated)));
        chain.mode = Mode::Tolerant;
        let decoded = chain.decode(truncated).map_err(|error| format!("{error:?}"))?;
        assert!(decoded.stages[0].incomplete);
        Ok(())
    }

    #[test]
    fn flate_limits_and_accounting() -> Result<(), Box<dyn std::error::Error>> {
        let data = b"bounded flate test";
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data)?;
        let encoded = encoder.finish()?;
        let decoded = chain(Filter::Flate, data.len() as u64).decode(&encoded).map_err(|error| format!("{error:?}"))?;
        assert_eq!(decoded.data, data);
        assert_eq!(decoded.stages[0].bytes_out, data.len() as u64);
        assert!(matches!(chain(Filter::Flate, data.len() as u64 - 1).decode(&encoded), Err(Error::OutputLimit)));
        Ok(())
    }

    #[test]
    fn flate_truncation_and_raw_recovery() -> Result<(), Box<dyn std::error::Error>> {
        let data = b"test raw deflate and truncated zlib stream";
        let mut raw = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        raw.write_all(data)?;
        let raw = raw.finish()?;
        let mut chain = chain(Filter::Flate, 100);
        assert!(chain.decode(&raw).is_err());
        chain.mode = Mode::Tolerant;
        assert_eq!(chain.decode(&raw).map_err(|error| format!("{error:?}"))?.data, data);
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(data)?;
        let zlib = zlib.finish()?;
        let truncated = &zlib[..zlib.len() - 2];
        chain.mode = Mode::Strict;
        assert!(matches!(chain.decode(truncated), Err(Error::Truncated)));
        chain.mode = Mode::Tolerant;
        let decoded = chain.decode(truncated).map_err(|error| format!("{error:?}"))?;
        assert!(decoded.stages[0].incomplete);
        Ok(())
    }
}
