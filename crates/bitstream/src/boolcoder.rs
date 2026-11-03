//! RFC 6386 §7 and VP9 specification v0.6 §9.2 boolean decoding.
//!
//! The high eight bits hold the arithmetic value; the remaining bits buffer input.
//! Only refill accesses the byte slice. Normalisation uses a leading-zero count.
//! VP8 permits implicit zero extension; VP9 checks marker and padding explicitly.

/// A malformed boolean partition or decoding request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolError {
    /// Initialisation or normalisation required bits beyond the partition.
    UnexpectedEof,
    /// The VP9 partition marker was not zero.
    InvalidMarker,
    /// Unconsumed VP9 padding contained a nonzero bit.
    NonzeroPadding,
    /// A tree was incomplete, cyclic, or addressed an invalid node.
    InvalidTree,
}

impl std::fmt::Display for BoolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::UnexpectedEof => "boolean partition exhausted",
            Self::InvalidMarker => "nonzero VP9 boolean marker",
            Self::NonzeroPadding => "nonzero VP9 boolean padding",
            Self::InvalidTree => "invalid boolean decoding tree",
        })
    }
}

impl std::error::Error for BoolError {}

/// An allocation-free boolean range decoder shared by VP8 and VP9.
pub struct BoolDecoder<'a> {
    data: &'a [u8],
    value: u64,
    count: i32,
    range: u32,
    error: Option<BoolError>,
}

impl<'a> BoolDecoder<'a> {
    /// Start a VP8 partition without consuming a marker. Missing input reads as zero.
    pub fn new(data: &'a [u8]) -> Self {
        let mut decoder = Self { data, value: 0, count: -8, range: 255, error: None };
        decoder.refill();
        decoder
    }

    /// Start a nonempty VP9 partition and require its equiprobable marker to be zero.
    pub fn new_vp9(data: &'a [u8]) -> Result<Self, BoolError> {
        if data.is_empty() {
            return Err(BoolError::UnexpectedEof);
        }
        let mut decoder = Self::new(data);
        if decoder.read_bool_half() {
            return Err(BoolError::InvalidMarker);
        }
        Ok(decoder)
    }

    #[inline]
    fn refill(&mut self) {
        let mut shift = 48 - self.count;
        if let Some(&bytes) = self.data.first_chunk::<8>() {
            let take = ((shift + 8) / 8) as usize;
            self.value |= (u64::from_be_bytes(bytes) >> (64 - 8 * take)) << (shift + 8 - 8 * take as i32);
            self.data = &self.data[take..];
            self.count += 8 * take as i32;
            return;
        }
        while shift >= 0 {
            let Some((&byte, rest)) = self.data.split_first() else {
                if self.count < 0 {
                    self.error.get_or_insert(BoolError::UnexpectedEof);
                    self.count = 56;
                }
                return;
            };
            self.value |= u64::from(byte) << shift;
            self.data = rest;
            self.count += 8;
            shift -= 8;
        }
    }

    /// Read a symbol whose probability of zero is `probability / 256`.
    #[inline(always)]
    pub fn read_bool(&mut self, probability: u8) -> bool {
        if self.count < 0 {
            self.refill();
        }
        let split = 1 + (((self.range - 1) * u32::from(probability)) >> 8);
        let threshold = u64::from(split) << 56;
        let bit = self.value >= threshold;
        let mask = u64::from(bit).wrapping_neg();
        self.value -= threshold & mask;
        self.range = ((self.range - split) & mask as u32) | (split & !(mask as u32));
        let shift = self.range.leading_zeros() - 24;
        self.range <<= shift;
        self.value <<= shift;
        self.count -= shift as i32;
        bit
    }

    /// Read an equiprobable bit, allowing constant folding of the split calculation.
    #[inline(always)]
    pub fn read_bool_half(&mut self) -> bool {
        self.read_bool(128)
    }

    /// Read an equiprobable flag.
    #[inline]
    pub fn read_flag(&mut self) -> bool {
        self.read_bool_half()
    }

    /// Read up to 32 equiprobable bits, most significant first. Widths above 32 are capped.
    #[inline]
    pub fn read_literal(&mut self, width: u32) -> u32 {
        let mut value = 0;
        for _ in 0..width.min(32) {
            value = (value << 1) | u32::from(self.read_bool_half());
        }
        value
    }

    /// Read up to 31 magnitude bits followed by a sign flag (one means negative).
    pub fn read_signed(&mut self, width: u32) -> i32 {
        let magnitude = self.read_literal(width.min(31)) as i32;
        if self.read_flag() { -magnitude } else { magnitude }
    }

    /// Read a presence flag and, when set, a signed magnitude.
    pub fn read_optional_signed(&mut self, width: u32) -> i32 {
        if self.read_flag() { self.read_signed(width) } else { 0 }
    }

    /// Decode a tree with positive node offsets and nonpositive, negated leaves.
    /// Malformed trees return zero and record [`BoolError::InvalidTree`].
    #[inline]
    pub fn read_tree(&mut self, tree: &[i8], probabilities: &[u8]) -> u8 {
        self.read_tree_from(tree, probabilities, 0)
    }

    /// Decode a subtree rooted at `start`, with bounded traversal even for cyclic trees.
    #[inline]
    pub fn read_tree_from(&mut self, tree: &[i8], probabilities: &[u8], start: usize) -> u8 {
        let mut node = start;
        for _ in 0..tree.len().min(128) {
            let Some(&probability) = probabilities.get(node >> 1) else { break };
            let branch = usize::from(self.read_bool(probability));
            let Some(&next) = tree.get(node.saturating_add(branch)) else { break };
            if next <= 0 {
                return next.unsigned_abs();
            }
            node = next as usize;
        }
        self.error.get_or_insert(BoolError::InvalidTree);
        0
    }

    /// VP9 `exit_bool`: reject overreads and require all unconsumed bits to be zero.
    /// Arithmetic value bits are not padding; buffered bits below them and unread bytes are.
    pub fn finish_vp9(&mut self) -> Result<(), BoolError> {
        if self.count < 0 {
            self.refill();
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.value << 8 != 0 || self.data.iter().any(|&byte| byte != 0) {
            return Err(BoolError::NonzeroPadding);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
