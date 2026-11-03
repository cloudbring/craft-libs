const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 { crc >> 1 ^ 0xedb8_8320 } else { crc >> 1 };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PngError {
    #[error("invalid PNG signature")]
    Signature,
    #[error("PNG chunk is truncated")]
    Truncated,
    #[error("PNG chunk exceeds {limit} bytes")]
    LengthLimit { limit: u32 },
    #[error("invalid PNG chunk type")]
    ChunkType,
    #[error("PNG chunk CRC mismatch")]
    Crc,
}

/// A checked chunk with borrowed bytes (not including the type or CRC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngChunk<'a> {
    pub kind: [u8; 4],
    pub data: &'a [u8],
}

/// Iterates chunks, validating their length, type and CRC before yielding.
pub struct PngChunks<'a> {
    remaining: &'a [u8],
    max_chunk_bytes: u32,
    done: bool,
}

impl<'a> PngChunks<'a> {
    pub fn new(bytes: &'a [u8], max_chunk_bytes: u32) -> Result<Self, PngError> {
        if !bytes.starts_with(SIGNATURE) {
            return Err(PngError::Signature);
        }
        Ok(Self { remaining: &bytes[SIGNATURE.len()..], max_chunk_bytes, done: false })
    }
}

impl<'a> Iterator for PngChunks<'a> {
    type Item = Result<PngChunk<'a>, PngError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.remaining.is_empty() {
            return None;
        }
        let result = self.parse();
        if result.is_err() {
            self.done = true;
        }
        Some(result)
    }
}

impl<'a> PngChunks<'a> {
    fn parse(&mut self) -> Result<PngChunk<'a>, PngError> {
        let header = self.remaining.get(..8).ok_or(PngError::Truncated)?;
        let length = u32::from_be_bytes(header[..4].try_into().map_err(|_| PngError::Truncated)?);
        if length > self.max_chunk_bytes {
            return Err(PngError::LengthLimit { limit: self.max_chunk_bytes });
        }
        let kind: [u8; 4] = header[4..8].try_into().map_err(|_| PngError::Truncated)?;
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(PngError::ChunkType);
        }
        let end = 8usize.checked_add(length as usize).and_then(|end| end.checked_add(4)).ok_or(PngError::Truncated)?;
        let chunk = self.remaining.get(..end).ok_or(PngError::Truncated)?;
        let payload = &chunk[8..end - 4];
        let expected = u32::from_be_bytes(chunk[end - 4..end].try_into().map_err(|_| PngError::Truncated)?);
        if crc32(&chunk[4..end - 4]) != expected {
            return Err(PngError::Crc);
        }
        self.remaining = &self.remaining[end..];
        Ok(PngChunk { kind, data: payload })
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc = crc >> 8 ^ CRC_TABLE[((crc ^ u32::from(byte)) & 0xff) as usize];
    }
    !crc
}
