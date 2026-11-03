use super::{Accounting, Error, Filter, Mode};

fn whitespace(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}

struct Output<'a> {
    bytes: Vec<u8>,
    max: u64,
    ratio: u64,
    accounting: &'a mut Accounting,
}

impl Output<'_> {
    fn push(&mut self, bytes: &[u8], consumed: usize) -> Result<(), Error> {
        self.accounting.bytes_in = consumed as u64;
        let size = (self.bytes.len() as u64).checked_add(bytes.len() as u64).ok_or(Error::OutputLimit)?;
        if size > self.max {
            return Err(Error::OutputLimit);
        }
        if size > (consumed as u64).saturating_mul(self.ratio) {
            return Err(Error::RatioLimit);
        }
        self.bytes.try_reserve_exact(bytes.len()).map_err(|_| Error::Allocation)?;
        self.bytes.extend_from_slice(bytes);
        self.accounting.bytes_out = size;
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn decode(filter: Filter, input: &[u8], max: u64, ratio: u64, mode: Mode) -> Result<(Vec<u8>, Accounting), Error> {
    decode_counted(filter, input, max, ratio, mode, &mut Accounting::default())
}

pub(super) fn decode_counted(
    filter: Filter,
    input: &[u8],
    max: u64,
    ratio: u64,
    mode: Mode,
    accounting: &mut Accounting,
) -> Result<(Vec<u8>, Accounting), Error> {
    let mut output = Output { bytes: Vec::new(), max, ratio, accounting };
    let (consumed, complete) = match filter {
        Filter::AsciiHex => hex(input, &mut output)?,
        Filter::Ascii85 => ascii85(input, &mut output)?,
        Filter::RunLength => run_length(input, &mut output)?,
        _ => return Err(Error::Corrupt),
    };
    if !complete && mode == Mode::Strict {
        return Err(Error::Truncated);
    }
    let accounting = Accounting { bytes_in: consumed as u64, bytes_out: output.bytes.len() as u64, incomplete: !complete };
    Ok((output.bytes, accounting))
}

fn hex(input: &[u8], output: &mut Output<'_>) -> Result<(usize, bool), Error> {
    let mut high = None;
    let mut consumed = 0;
    let mut complete = false;
    for byte in input.iter().copied() {
        consumed += 1;
        output.accounting.bytes_in = consumed as u64;
        if whitespace(byte) {
            continue;
        }
        if byte == b'>' {
            complete = true;
            break;
        }
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return Err(Error::Corrupt),
        };
        if let Some(value) = high.take() {
            output.push(&[(value << 4) | digit], consumed)?;
        } else {
            high = Some(digit);
        }
    }
    if let Some(value) = high {
        output.push(&[value << 4], consumed)?;
    }
    Ok((consumed, complete))
}

fn ascii85(input: &[u8], output: &mut Output<'_>) -> Result<(usize, bool), Error> {
    let mut consumed = 0;
    let mut accumulator = 0u64;
    let mut count = 0;
    let mut complete = false;
    while consumed < input.len() {
        let byte = input[consumed];
        consumed += 1;
        output.accounting.bytes_in = consumed as u64;
        if whitespace(byte) {
            continue;
        }
        match byte {
            b'!'..=b'u' => {
                accumulator = accumulator * 85 + u64::from(byte - b'!');
                count += 1;
                if count == 5 {
                    let value = u32::try_from(accumulator).map_err(|_| Error::Corrupt)?;
                    output.push(&value.to_be_bytes(), consumed)?;
                    accumulator = 0;
                    count = 0;
                }
            }
            b'z' if count == 0 => output.push(&[0; 4], consumed)?,
            b'~' if input.get(consumed) == Some(&b'>') => {
                consumed += 1;
                complete = true;
                break;
            }
            b'~' if consumed == input.len() => break,
            _ => return Err(Error::Corrupt),
        }
    }
    if count == 1 {
        return Err(Error::Corrupt);
    }
    if count > 1 {
        for _ in count..5 {
            accumulator = accumulator * 85 + 84;
        }
        let value = u32::try_from(accumulator).map_err(|_| Error::Corrupt)?;
        output.push(&value.to_be_bytes()[..count - 1], consumed)?;
    }
    Ok((consumed, complete))
}

fn run_length(input: &[u8], output: &mut Output<'_>) -> Result<(usize, bool), Error> {
    let mut consumed = 0usize;
    while consumed < input.len() {
        let length = input[consumed];
        consumed += 1;
        output.accounting.bytes_in = consumed as u64;
        if length == 128 {
            return Ok((consumed, true));
        }
        if length < 128 {
            let end = consumed.saturating_add(usize::from(length) + 1).min(input.len());
            output.push(&input[consumed..end], end)?;
            if end - consumed != usize::from(length) + 1 {
                return Ok((end, false));
            }
            consumed = end;
        } else if let Some(byte) = input.get(consumed).copied() {
            consumed += 1;
            let run = [byte; 128];
            output.push(&run[..257 - usize::from(length)], consumed)?;
        } else {
            return Ok((consumed, false));
        }
    }
    Ok((consumed, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specification_vectors() -> Result<(), Error> {
        assert_eq!(decode(Filter::AsciiHex, b"61 62 6>", 10, 10, Mode::Strict)?.0, b"ab`");
        assert_eq!(decode(Filter::Ascii85, b"87cURD]j7BEbo80~>", 20, 10, Mode::Strict)?.0, b"Hello world!");
        assert_eq!(decode(Filter::Ascii85, b"z~>", 4, 10, Mode::Strict)?.0, [0; 4]);
        assert_eq!(decode(Filter::RunLength, &[2, b'a', b'b', b'c', 254, b'x', 128], 6, 10, Mode::Strict)?.0, b"abcxxx");
        Ok(())
    }

    #[test]
    fn truncation_and_bad_encodings() -> Result<(), Error> {
        assert!(matches!(decode(Filter::AsciiHex, b"AB", 10, 10, Mode::Strict), Err(Error::Truncated)));
        assert!(decode(Filter::AsciiHex, b"AB", 10, 10, Mode::Tolerant)?.1.incomplete);
        assert!(matches!(decode(Filter::Ascii85, b"!z~>", 10, 10, Mode::Strict), Err(Error::Corrupt)));
        assert!(matches!(decode(Filter::RunLength, &[2, b'a'], 10, 10, Mode::Strict), Err(Error::Truncated)));
        assert!(decode(Filter::RunLength, &[2, b'a'], 10, 10, Mode::Tolerant)?.1.incomplete);
        Ok(())
    }
}
