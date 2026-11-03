use std::cell::Cell;
use std::rc::Rc;

/// Per-file resource ceilings, shared by plain records, EMF+ and nested images.
/// Exceeding a ceiling returns [`crate::Error::LimitExceeded`], never a partial success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum records visited, including nested files and EMF+ comment records.
    pub max_records: usize,
    /// Maximum scene items emitted, including nested pictures.
    pub max_items: usize,
    /// Maximum aggregate decoded bitmap pixels (including image and brush objects).
    pub max_bitmap_pixels: usize,
    /// Maximum input length and conservative cumulative allocation budget, independently.
    /// Record buffers, scene data, saved state and decoded pixels are charged before use.
    /// Allocator bookkeeping and fixed parser state are not included.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_records: 1_000_000, max_items: crate::MAX_ITEMS, max_bitmap_pixels: crate::MAX_PIXELS, max_bytes: crate::MAX_BYTES }
    }
}

#[derive(Default)]
struct Counts {
    records: Cell<usize>,
    items: Cell<usize>,
    pixels: Cell<usize>,
    bytes: Cell<usize>,
    exceeded: Cell<bool>,
}

#[derive(Clone)]
pub(crate) struct Budget {
    pub limits: Limits,
    pub depth: usize,
    counts: Rc<Counts>,
}

impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self { limits, depth: 0, counts: Rc::default() }
    }

    pub fn exceeded(&self) -> bool {
        self.counts.exceeded.get()
    }

    pub fn fail(&self) -> bool {
        self.counts.exceeded.set(true);
        false
    }

    fn take(&self, counter: &Cell<usize>, amount: usize, limit: usize) -> bool {
        match counter.get().checked_add(amount) {
            Some(total) if total <= limit && !self.exceeded() => {
                counter.set(total);
                true
            }
            _ => self.fail(),
        }
    }

    pub fn bytes(&self, amount: usize) -> bool {
        self.take(&self.counts.bytes, amount, self.limits.max_bytes)
    }

    pub fn remaining_bytes(&self) -> usize {
        self.limits.max_bytes.saturating_sub(self.counts.bytes.get())
    }

    pub fn record(&self, size: usize) -> bool {
        self.take(&self.counts.records, 1, self.limits.max_records) && self.bytes(size.saturating_mul(16).saturating_add(128))
    }

    pub fn item(&self, size: usize) -> bool {
        self.take(&self.counts.items, 1, self.limits.max_items) && self.bytes(size.saturating_mul(2))
    }

    pub fn bitmap(&self, width: usize, height: usize) -> bool {
        let Some(pixels) = width.checked_mul(height) else { return self.fail() };
        self.take(&self.counts.pixels, pixels, self.limits.max_bitmap_pixels) && self.bytes(pixels.saturating_mul(12))
    }

    pub fn nested(&self) -> Option<Self> {
        if self.depth >= 3 {
            self.fail();
            None
        } else {
            Some(Self { depth: self.depth + 1, ..self.clone() })
        }
    }
}
