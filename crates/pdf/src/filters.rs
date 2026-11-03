//! Bounded PDF stream decoding primitives.
//!
//! This module is independent of the reader and rasterizer features.

use craft_archive::{BoundedInflate, InflateError, Wrapper};
use weezl::{BitOrder, LzwStatus, decode::Decoder};

mod encode;
mod predictor;
mod simple;
pub use encode::{EncodeParams, encode, encode_chain};
pub use predictor::Predictor;

/// Resource ceilings. `max_output` bounds the sum of bytes emitted by all
/// decoders, before predictor reversal (including PNG row tags). Predictor
/// reversal is not charged again. `max_input` bounds each stage's input;
/// `max_ratio` bounds each decoder's output relative to its consumed input;
/// `max_depth` bounds the number of filters.
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

/// Flate framing, independent of incomplete-stream handling.
///
/// pdfcraft uses header detection in both strict and tolerant decoding; use
/// `Auto` with the corresponding [`Mode`]. Unlike pdfcraft, zlib framing here
/// validates the Adler-32 trailer and does not retry a valid-looking header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlateWrapper {
    Zlib,
    Raw,
    /// Select zlib for a valid RFC 1950 header, otherwise raw DEFLATE.
    Auto,
}

/// A supported stream decoding stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Flate,
    /// Explicit framing; `Auto` encoding produces zlib-wrapped output.
    FlateWithWrapper {
        wrapper: FlateWrapper,
        predictor: Option<Predictor>,
    },
    Lzw {
        early_change: bool,
    },
    FlatePredictor(Predictor),
    LzwPredictor {
        early_change: bool,
        predictor: Predictor,
    },
    AsciiHex,
    Ascii85,
    RunLength,
    /// Preserve encoded JPEG bytes for the application's image decoder.
    Dct,
    /// CCITT fax data is not decoded by this feature yet.
    Ccitt(CcittParams),
}

/// PDF CCITT fax decoding parameters, retained for future decoder support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CcittParams {
    pub k: i32,
    pub columns: u32,
    pub rows: u32,
    pub end_of_line: bool,
    pub encoded_byte_align: bool,
    pub end_of_block: bool,
    pub black_is_one: bool,
}

impl Default for CcittParams {
    fn default() -> Self {
        Self { k: 0, columns: 1728, rows: 0, end_of_line: false, encoded_byte_align: false, end_of_block: true, black_is_one: false }
    }
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
    /// The requested codec is deliberately not implemented.
    Unsupported(&'static str),
}

/// Result of decoding all stages in the supplied order.
#[derive(Debug)]
pub struct Decoded {
    pub data: Vec<u8>,
    pub stages: Vec<Accounting>,
}

/// Accounting from completed stages and the currently failing stage, if any.
#[derive(Debug, Default)]
pub struct ChainAccounting {
    pub stages: Vec<Accounting>,
    /// Sum of input bytes consumed by all stage attempts.
    pub total_input: u64,
    /// Sum of stage output bytes charged against `Limits::max_output`.
    pub total: u64,
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
        self.decode_with_accounting(input).0
    }

    /// Decode while retaining completed work on both success and failure.
    pub fn decode_with_accounting(&self, input: &[u8]) -> (Result<Decoded, Error>, ChainAccounting) {
        let mut accounting = ChainAccounting::default();
        let result = self.decode_inner(input, &mut accounting);
        (result, accounting)
    }

    fn decode_inner(&self, input: &[u8], chain: &mut ChainAccounting) -> Result<Decoded, Error> {
        if self.filters.len() > self.limits.max_depth {
            return Err(Error::DepthLimit);
        }
        if input.len() as u64 > self.limits.max_input {
            return Err(Error::InputLimit);
        }
        let mut current = Vec::new();
        current.try_reserve_exact(input.len()).map_err(|_| Error::Allocation)?;
        current.extend_from_slice(input);
        chain.stages.try_reserve_exact(self.filters.len()).map_err(|_| Error::Allocation)?;
        for filter in &self.filters {
            if current.len() as u64 > self.limits.max_input {
                return Err(Error::InputLimit);
            }
            let remaining = self.limits.max_output.saturating_sub(chain.total);
            chain.stages.push(Accounting::default());
            let stage = chain.stages.last_mut().ok_or(Error::Allocation)?;
            let result = (|| -> Result<(Vec<u8>, Option<Predictor>), Error> {
                let (mut output, mut accounting, predictor) = match *filter {
                    Filter::Flate => {
                        let (output, accounting) = self.flate(&current, remaining, stage)?;
                        (output, accounting, None)
                    }
                    Filter::FlateWithWrapper { wrapper, predictor } => {
                        let wrapper = match wrapper {
                            FlateWrapper::Zlib => Wrapper::Zlib,
                            FlateWrapper::Raw => Wrapper::Raw,
                            FlateWrapper::Auto if has_zlib_header(&current) => Wrapper::Zlib,
                            FlateWrapper::Auto => Wrapper::Raw,
                        };
                        let (output, accounting) = self.inflate(&current, remaining, wrapper, stage)?;
                        (output, accounting, predictor)
                    }
                    Filter::Lzw { early_change } => {
                        let (output, accounting) = self.lzw(&current, remaining, early_change, stage)?;
                        (output, accounting, None)
                    }
                    Filter::FlatePredictor(predictor) => {
                        let (output, accounting) = self.flate(&current, remaining, stage)?;
                        (output, accounting, Some(predictor))
                    }
                    Filter::LzwPredictor { early_change, predictor } => {
                        let (output, accounting) = self.lzw(&current, remaining, early_change, stage)?;
                        (output, accounting, Some(predictor))
                    }
                    Filter::AsciiHex | Filter::Ascii85 | Filter::RunLength => {
                        let (output, accounting) = simple::decode_counted(*filter, &current, remaining, self.limits.max_ratio, self.mode, stage)?;
                        (output, accounting, None)
                    }
                    Filter::Dct => {
                        let size = current.len() as u64;
                        if size > remaining {
                            return Err(Error::OutputLimit);
                        }
                        if size > size.saturating_mul(self.limits.max_ratio) {
                            return Err(Error::RatioLimit);
                        }
                        let mut copy = Vec::new();
                        copy.try_reserve_exact(current.len()).map_err(|_| Error::Allocation)?;
                        copy.extend_from_slice(&current);
                        (copy, Accounting { bytes_in: size, bytes_out: size, incomplete: false }, None)
                    }
                    Filter::Ccitt(_) => return Err(Error::Unsupported("CCITTFaxDecode")),
                };
                *stage = accounting;
                if let Some(predictor) = predictor {
                    let (decoded, incomplete) = predictor::decode(predictor, output, self.mode)?;
                    output = decoded;
                    accounting.incomplete |= incomplete;
                    stage.incomplete = accounting.incomplete;
                }
                Ok((output, predictor))
            })();
            chain.total_input = chain.total_input.saturating_add(stage.bytes_in);
            chain.total = chain.total.saturating_add(stage.bytes_out);
            let (output, _) = result?;
            current = output;
        }
        if self.filters.is_empty() && current.len() as u64 > self.limits.max_output {
            return Err(Error::OutputLimit);
        }
        Ok(Decoded { data: current, stages: chain.stages.clone() })
    }

    fn flate(&self, input: &[u8], max_output: u64, stage: &mut Accounting) -> Result<(Vec<u8>, Accounting), Error> {
        match self.inflate(input, max_output, Wrapper::Zlib, stage) {
            Err(Error::Corrupt | Error::Truncated) if self.mode == Mode::Tolerant => {
                let previous = *stage;
                let remaining = max_output.saturating_sub(previous.bytes_out);
                *stage = Accounting::default();
                let result = self.inflate(input, remaining, Wrapper::Raw, stage);
                stage.bytes_in = stage.bytes_in.saturating_add(previous.bytes_in);
                stage.bytes_out = stage.bytes_out.saturating_add(previous.bytes_out);
                result.map(|(output, _)| (output, *stage))
            }
            result => result,
        }
    }

    fn inflate(&self, input: &[u8], max_output: u64, wrapper: Wrapper, stage: &mut Accounting) -> Result<(Vec<u8>, Accounting), Error> {
        let limits = craft_archive::Limits { max_output, max_input: self.limits.max_input, max_ratio: self.limits.max_ratio, wrapper };
        let mut decoder = BoundedInflate::new(input, limits).map_err(InflateError::Io).map_err(Error::Inflate)?;
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut incomplete = false;
        loop {
            let remaining = max_output.saturating_sub(decoder.accounting().bytes_out);
            let size = buffer.len().min(usize::try_from(remaining.max(1)).unwrap_or(usize::MAX));
            let read = decoder.read_chunk(&mut buffer[..size]);
            let progress = decoder.accounting();
            stage.bytes_in = progress.bytes_in;
            stage.bytes_out = progress.bytes_out;
            let count = match read {
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

    fn lzw(&self, input: &[u8], max_output: u64, early_change: bool, stage: &mut Accounting) -> Result<(Vec<u8>, Accounting), Error> {
        let mut decoder = if early_change { Decoder::with_tiff_size_switch(BitOrder::Msb, 8) } else { Decoder::new(BitOrder::Msb, 8) };
        let mut output = Vec::new();
        let mut consumed = 0usize;
        let mut buffer = [0u8; 8192];
        loop {
            let result = decoder.decode_bytes(&input[consumed..], &mut buffer);
            consumed += result.consumed_in;
            stage.bytes_in = consumed as u64;
            stage.bytes_out = (output.len() as u64).saturating_add(result.consumed_out as u64);
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

fn has_zlib_header(input: &[u8]) -> bool {
    if let [cmf, flg, ..] = input { cmf & 15 == 8 && cmf >> 4 <= 7 && u16::from_be_bytes([*cmf, *flg]) % 31 == 0 } else { false }
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

    #[test]
    fn boundaries_for_each_filter() -> Result<(), Box<dyn std::error::Error>> {
        let payload = b"abcd";
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(payload)?;
        let zlib = zlib.finish()?;
        let lzw = Encoder::new(BitOrder::Msb, 8).encode(payload)?;
        let cases: [(Filter, &[u8]); 7] = [
            (Filter::Flate, &zlib),
            (Filter::Lzw { early_change: false }, &lzw),
            (Filter::FlatePredictor(Predictor::default()), &zlib),
            (Filter::LzwPredictor { early_change: false, predictor: Predictor::default() }, &lzw),
            (Filter::AsciiHex, b"61626364>"),
            (Filter::Ascii85, b"@:E_W~>"),
            (Filter::RunLength, &[3, b'a', b'b', b'c', b'd', 128]),
        ];
        for (filter, encoded) in cases {
            let mut test = chain(filter, 4);
            test.limits.max_input = encoded.len() as u64;
            let result = test.decode(encoded).map_err(|error| format!("{filter:?}: {error:?}"))?;
            assert_eq!(result.data, payload, "{filter:?}");
            test.limits.max_output = 3;
            assert!(matches!(test.decode(encoded), Err(Error::OutputLimit)), "{filter:?}");
            test.limits.max_output = 4;
            test.limits.max_input -= 1;
            assert!(matches!(test.decode(encoded), Err(Error::InputLimit)), "{filter:?}");
            test.limits.max_input += 1;
            test.limits.max_ratio = 0;
            assert!(matches!(test.decode(encoded), Err(Error::RatioLimit)), "{filter:?}");
            test.limits.max_depth = 0;
            assert!(matches!(test.decode(encoded), Err(Error::DepthLimit)), "{filter:?}");
        }
        let encoded = [253, b'x', 128];
        let mut test = chain(Filter::RunLength, 4);
        test.limits.max_ratio = 2;
        assert!(test.decode(&encoded).is_ok());
        test.limits.max_ratio = 1;
        assert!(matches!(test.decode(&encoded), Err(Error::RatioLimit)));
        Ok(())
    }

    #[test]
    fn chain_cumulative_budget_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let mut test = chain(Filter::AsciiHex, 7);
        test.filters.push(Filter::AsciiHex);
        test.limits.max_depth = 2;
        let decoded = test.decode(b"343134323E>").map_err(|error| format!("{error:?}"))?;
        assert_eq!(decoded.data, b"AB");
        assert_eq!(decoded.stages.iter().map(|stage| stage.bytes_out).sum::<u64>(), 7);
        test.limits.max_output = 6;
        assert!(matches!(test.decode(b"343134323E>"), Err(Error::OutputLimit)));
        Ok(())
    }

    #[test]
    fn nested_flate_bomb_stops_at_ratio_limit() -> Result<(), Box<dyn std::error::Error>> {
        fn compress(data: &[u8]) -> std::io::Result<Vec<u8>> {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(data)?;
            encoder.finish()
        }
        let inner = compress(&vec![0; 1_000_000])?;
        let outer = compress(&inner)?;
        assert!(1_000_000 / inner.len() >= 1000);
        let mut test = chain(Filter::Flate, 2_000_000);
        test.limits.max_ratio = 50;
        assert_eq!(test.decode(&outer).map_err(|error| format!("{error:?}"))?.data, inner);
        test.filters.push(Filter::Flate);
        test.limits.max_depth = 2;
        assert!(matches!(test.decode(&outer), Err(Error::RatioLimit)));
        Ok(())
    }

    #[test]
    fn image_filters_have_explicit_behavior() -> Result<(), Box<dyn std::error::Error>> {
        let jpeg = [0xff, 0xd8, 0xff, 0xd9];
        let result = chain(Filter::Dct, 4).decode(&jpeg).map_err(|error| format!("{error:?}"))?;
        assert_eq!(result.data, jpeg);
        assert_eq!(result.stages[0].bytes_out, 4);
        assert!(matches!(chain(Filter::Dct, 3).decode(&jpeg), Err(Error::OutputLimit)));
        assert!(matches!(chain(Filter::Ccitt(CcittParams::default()), 4).decode(&jpeg), Err(Error::Unsupported("CCITTFaxDecode"))));
        Ok(())
    }

    #[test]
    fn pdfcraft_bomb_thresholds_and_failure_accounting() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0; 16 << 20];
        let encoded = encode(Filter::Flate, &data, EncodeParams::default()).map_err(|error| format!("{error:?}"))?;
        let mut test = chain(Filter::Flate, 0);
        test.limits.max_input = u64::MAX;
        test.limits.max_ratio = u64::MAX;
        for limit in [0, 1 << 20, (16 << 20) - 1] {
            test.limits.max_output = limit;
            let (result, accounting) = test.decode_with_accounting(&encoded);
            assert!(matches!(result, Err(Error::OutputLimit)));
            assert_eq!(accounting.total, limit);
            assert_eq!(accounting.stages[0].bytes_out, limit);
        }
        test.limits.max_output = 16 << 20;
        assert_eq!(test.decode(&encoded).map_err(|error| format!("{error:?}"))?.data, data);
        let inner = encode(Filter::Flate, &vec![0; 32 << 20], EncodeParams::default()).map_err(|error| format!("{error:?}"))?;
        let outer = encode(Filter::Flate, &inner, EncodeParams::default()).map_err(|error| format!("{error:?}"))?;
        test.filters.push(Filter::Flate);
        test.limits.max_depth = 2;
        test.limits.max_output = 1 << 20;
        let (result, accounting) = test.decode_with_accounting(&outer);
        assert!(matches!(result, Err(Error::OutputLimit)));
        assert_eq!(accounting.stages.len(), 2);
        assert_eq!(accounting.stages[0].bytes_out, inner.len() as u64);
        assert_eq!(accounting.stages[1].bytes_out, (1 << 20) - inner.len() as u64);
        assert_eq!(accounting.total, 1 << 20);
        Ok(())
    }

    #[test]
    fn predictor_count_and_encoder_compatibility() -> Result<(), Box<dyn std::error::Error>> {
        let params = EncodeParams::default();
        for (filter, input, expected) in [
            (Filter::AsciiHex, &b"Hello"[..], &b"48656C6C6F>"[..]),
            (Filter::Ascii85, &b"Man "[..], &b"9jqo^~>"[..]),
            (Filter::Ascii85, &b"M"[..], &b"9`~>"[..]),
            (Filter::RunLength, &b"abcxxx"[..], &[2, b'a', b'b', b'c', 254, b'x', 128][..]),
        ] {
            assert_eq!(encode(filter, input, params).map_err(|error| format!("{error:?}"))?, expected);
        }
        let data: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let filters = [Filter::Flate, Filter::AsciiHex, Filter::Ascii85, Filter::RunLength];
        for filter in filters {
            let encoded = encode(filter, &data, params).map_err(|error| format!("{error:?}"))?;
            if filter == Filter::Flate {
                assert_eq!(&encoded[..2], &[0x78, 0x9c]);
            }
            let mut test = chain(filter, 100_000);
            test.limits.max_input = 100_000;
            assert_eq!(test.decode(&encoded).map_err(|error| format!("{error:?}"))?.data, data);
        }
        for predictor in [2, 10, 11, 12, 13, 14, 15] {
            let filter = Filter::FlatePredictor(Predictor { predictor, columns: 16, ..Predictor::default() });
            let encoded = encode(filter, &data, params).map_err(|error| format!("{error:?}"))?;
            let mut test = chain(filter, 100_000);
            test.limits.max_input = 100_000;
            let decoded = test.decode(&encoded).map_err(|error| format!("{error:?}"))?;
            assert_eq!(decoded.data, data);
            assert_eq!(decoded.stages[0].bytes_out, (data.len() + if predictor == 2 { 0 } else { data.len() / 16 }) as u64);
        }
        let predictor = Filter::FlatePredictor(Predictor { predictor: 12, ..Predictor::default() });
        let rows = vec![7; 1 << 16];
        let encoded = encode(predictor, &rows, params).map_err(|error| format!("{error:?}"))?;
        let mut test = chain(predictor, 2 << 16);
        test.limits.max_ratio = u64::MAX;
        let decoded = test.decode(&encoded).map_err(|error| format!("{error:?}"))?;
        assert_eq!(decoded.data, rows);
        assert_eq!(decoded.stages[0].bytes_out, 2 << 16);
        let stages = [(Filter::Ascii85, params), (Filter::Flate, params)];
        let encoded = encode_chain(&stages, &data).map_err(|error| format!("{error:?}"))?;
        let test = FilterChain {
            filters: stages.iter().map(|stage| stage.0).collect(),
            limits: Limits { max_input: 100_000, max_output: 100_000, max_ratio: 100, max_depth: 2 },
            mode: Mode::Strict,
        };
        assert_eq!(test.decode(&encoded).map_err(|error| format!("{error:?}"))?.data, data);
        Ok(())
    }

    #[test]
    fn failure_preserves_stage_progress() -> Result<(), Box<dyn std::error::Error>> {
        let test = chain(Filter::AsciiHex, 100);
        let (result, progress) = test.decode_with_accounting(b"4142G>");
        assert!(matches!(result, Err(Error::Corrupt)));
        assert_eq!(progress.stages[0].bytes_in, 5);
        assert_eq!(progress.total, 2);
        let predictor = Predictor { predictor: 12, ..Predictor::default() };
        let bad = encode(Filter::Flate, &[0, 7, 9, 7], EncodeParams::default()).map_err(|error| format!("{error:?}"))?;
        let (result, progress) = chain(Filter::FlatePredictor(predictor), 100).decode_with_accounting(&bad);
        assert!(matches!(result, Err(Error::Corrupt)));
        assert_eq!(progress.total, 4);
        assert_eq!(progress.stages[0].bytes_out, 4);
        Ok(())
    }

    #[test]
    #[ignore = "50 MiB performance benchmark; run explicitly in release mode"]
    fn bench_flate_50_mib() -> Result<(), Box<dyn std::error::Error>> {
        use std::time::Instant;

        const SIZE: usize = 50 * 1024 * 1024;
        let input = vec![b'a'; SIZE];
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&input)?;
        let encoded = encoder.finish()?;
        let mut test = chain(Filter::Flate, SIZE as u64);
        test.limits.max_input = encoded.len() as u64;
        test.limits.max_ratio = 2000;
        let start = Instant::now();
        let decoded = test.decode(std::hint::black_box(&encoded)).map_err(|error| format!("{error:?}"))?;
        let elapsed = start.elapsed();
        assert_eq!(decoded.data, input);
        eprintln!("50 MiB Flate decoded in {elapsed:?}");
        Ok(())
    }
}
