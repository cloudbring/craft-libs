use std::fmt;
use std::io::{self, BufRead, Read};

/// Which DEFLATE envelope to decode. Auto recognizes gzip and zlib headers;
/// otherwise it treats the input as raw DEFLATE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrapper {
    Auto,
    Raw,
    Zlib,
    Gzip,
}

/// Hard byte ceilings; ratio is maximum output bytes per consumed input byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_output: u64,
    pub max_input: u64,
    pub max_ratio: u64,
    pub wrapper: Wrapper,
}

/// Bytes actually consumed by the decoder and delivered to the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accounting {
    pub bytes_in: u64,
    pub bytes_out: u64,
}

/// Failures are separated from the input I/O error.
#[derive(Debug, thiserror::Error)]
pub enum InflateError {
    #[error("inflated output exceeds {limit} bytes")]
    OutputLimit { limit: u64 },
    #[error("compressed input exceeds {limit} bytes")]
    InputLimit { limit: u64 },
    #[error("inflated output exceeds the expansion ratio")]
    RatioLimit,
    #[error("compressed stream is truncated")]
    Truncated,
    #[error("compressed stream is corrupt")]
    Corrupt,
    #[error("input I/O error: {0}")]
    Io(#[source] io::Error),
}

struct Input<R> {
    inner: io::Chain<io::Cursor<Vec<u8>>, R>,
    buffer: [u8; 8192],
    start: usize,
    end: usize,
    consumed: u64,
    limit: u64,
}

#[derive(Debug)]
struct InputLimitMarker;

impl fmt::Display for InputLimitMarker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("compressed input limit reached")
    }
}

impl std::error::Error for InputLimitMarker {}

impl<R: Read> Input<R> {
    fn new(prefix: Vec<u8>, inner: R, limit: u64) -> Self {
        Self { inner: io::Cursor::new(prefix).chain(inner), buffer: [0; 8192], start: 0, end: 0, consumed: 0, limit }
    }
}

impl<R: Read> Read for Input<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let length = available.len().min(output.len());
        output[..length].copy_from_slice(&available[..length]);
        self.consume(length);
        Ok(length)
    }
}

impl<R: Read> BufRead for Input<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.start == self.end {
            if self.consumed == self.limit {
                let mut probe = [0];
                if self.inner.read(&mut probe)? != 0 {
                    return Err(io::Error::other(InputLimitMarker));
                }
                return Ok(&[]);
            }
            self.start = 0;
            let length = usize::try_from(self.limit - self.consumed).unwrap_or(usize::MAX).min(self.buffer.len());
            self.end = self.inner.read(&mut self.buffer[..length])?;
        }
        Ok(&self.buffer[self.start..self.end])
    }

    fn consume(&mut self, amount: usize) {
        let amount = amount.min(self.end - self.start);
        self.start += amount;
        self.consumed += amount as u64;
    }
}

enum Decoder<R: Read> {
    Raw(flate2::bufread::DeflateDecoder<Input<R>>),
    Zlib(flate2::bufread::ZlibDecoder<Input<R>>),
    Gzip(flate2::bufread::MultiGzDecoder<Input<R>>),
}

impl<R: Read> Decoder<R> {
    fn consumed(&self) -> u64 {
        match self {
            Self::Raw(decoder) => decoder.get_ref().consumed,
            Self::Zlib(decoder) => decoder.get_ref().consumed,
            Self::Gzip(decoder) => decoder.get_ref().consumed,
        }
    }

    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Raw(decoder) => decoder.read(buffer),
            Self::Zlib(decoder) => decoder.read(buffer),
            Self::Gzip(decoder) => decoder.read(buffer),
        }
    }
}

/// Streaming bounded decoder. Use `read_chunk` for typed errors and `accounting`
/// even after a limit failure. `Read` is also implemented for composition.
pub struct BoundedInflate<R: Read> {
    decoder: Decoder<R>,
    limits: Limits,
    accounting: Accounting,
    finished: bool,
    failed: bool,
}

impl<R: Read> BoundedInflate<R> {
    /// Creates a decoder; Auto peeks at up to two bytes without losing them.
    pub fn new(mut source: R, limits: Limits) -> io::Result<Self> {
        let mut head = [0; 2];
        let mut length = 0;
        if limits.wrapper == Wrapper::Auto {
            while length < 2 && (length as u64) < limits.max_input {
                let count = source.read(&mut head[length..])?;
                if count == 0 {
                    break;
                }
                length += count;
            }
        }
        let wrapper = if limits.wrapper == Wrapper::Auto {
            if length == 2 && head == [0x1f, 0x8b] {
                Wrapper::Gzip
            } else if length == 2 && is_zlib(head) {
                Wrapper::Zlib
            } else {
                Wrapper::Raw
            }
        } else {
            limits.wrapper
        };
        let input = Input::new(head[..length].to_vec(), source, limits.max_input);
        let decoder = match wrapper {
            Wrapper::Raw | Wrapper::Auto => Decoder::Raw(flate2::bufread::DeflateDecoder::new(input)),
            Wrapper::Zlib => Decoder::Zlib(flate2::bufread::ZlibDecoder::new(input)),
            Wrapper::Gzip => Decoder::Gzip(flate2::bufread::MultiGzDecoder::new(input)),
        };
        Ok(Self { decoder, limits, accounting: Accounting::default(), finished: false, failed: false })
    }

    pub fn accounting(&self) -> Accounting {
        self.accounting
    }

    /// Reads a bounded chunk. After an error the decoder cannot be resumed.
    pub fn read_chunk(&mut self, output: &mut [u8]) -> Result<usize, InflateError> {
        if self.failed {
            return Err(InflateError::Corrupt);
        }
        if self.finished || output.is_empty() {
            return Ok(0);
        }
        let remaining = self.limits.max_output - self.accounting.bytes_out;
        let size = output.len().min(8192).min(usize::try_from(remaining.saturating_add(1)).unwrap_or(usize::MAX));
        let mut buffer = [0; 8192];
        let count = match self.decoder.read(&mut buffer[..size]) {
            Ok(count) => count,
            Err(error) => {
                self.accounting.bytes_in = self.decoder.consumed();
                self.failed = true;
                return Err(classify(error, self.limits.max_input));
            }
        };
        self.accounting.bytes_in = self.decoder.consumed();
        if count == 0 {
            self.finished = true;
            return Ok(0);
        }
        if count as u64 > remaining {
            self.failed = true;
            return Err(InflateError::OutputLimit { limit: self.limits.max_output });
        }
        let total = self.accounting.bytes_out + count as u64;
        if total > self.accounting.bytes_in.saturating_mul(self.limits.max_ratio) {
            self.failed = true;
            return Err(InflateError::RatioLimit);
        }
        output[..count].copy_from_slice(&buffer[..count]);
        self.accounting.bytes_out = total;
        Ok(count)
    }
}

impl<R: Read> Read for BoundedInflate<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.read_chunk(output).map_err(io::Error::other)
    }
}

fn is_zlib(head: [u8; 2]) -> bool {
    head[0] & 0x0f == 8 && head[0] >> 4 <= 7 && u16::from_be_bytes(head).is_multiple_of(31)
}

fn classify(error: io::Error, limit: u64) -> InflateError {
    if error.get_ref().is_some_and(|cause| cause.is::<InputLimitMarker>()) {
        InflateError::InputLimit { limit }
    } else if error.kind() == io::ErrorKind::UnexpectedEof {
        InflateError::Truncated
    } else if error.kind() == io::ErrorKind::InvalidInput || error.kind() == io::ErrorKind::InvalidData {
        InflateError::Corrupt
    } else {
        InflateError::Io(error)
    }
}

/// Inflates a complete in-memory stream within the supplied limits.
pub fn inflate_bounded(data: &[u8], limits: Limits) -> Result<Vec<u8>, InflateError> {
    let mut reader = BoundedInflate::new(data, limits).map_err(InflateError::Io)?;
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read_chunk(&mut buffer)?;
        if count == 0 {
            return Ok(output);
        }
        output.try_reserve_exact(count).map_err(|_| InflateError::OutputLimit { limit: limits.max_output })?;
        output.extend_from_slice(&buffer[..count]);
    }
}
