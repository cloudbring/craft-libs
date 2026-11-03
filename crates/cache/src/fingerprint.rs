//! Stable FNV-1a hashing. Switch to craft-hash when it is available on main.

/// FNV-1a 64-bit state. Values use explicitly documented encodings, not `Hash`.
#[derive(Clone, Debug)]
pub struct FingerprintHasher(u64);

impl Default for FingerprintHasher {
    fn default() -> Self {
        Self(0xcbf29ce484222325)
    }
}

impl FingerprintHasher {
    /// Append bytes to the fingerprint.
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }
    /// Return the current fingerprint.
    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// Deterministic fingerprint encoding; derived structs encode every field in declaration order.
///
/// Integers are little-endian, strings and slices are length-prefixed with a u64,
/// and enum-like wrappers use a one-byte discriminant. Changing a type's fields
/// changes its fingerprint format. Avoid relying on hash equality as proof of equality.
///
/// ```compile_fail
/// #[derive(craft_cache::Fingerprint)]
/// struct Key { field: std::cell::Cell<u32> }
/// ```
pub trait Fingerprint {
    /// Append this value's stable encoding.
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher);
    /// Hash this value from the FNV-1a offset basis.
    fn fingerprint(&self) -> u64 {
        let mut hasher = FingerprintHasher::default();
        self.write_fingerprint(&mut hasher);
        hasher.finish()
    }
}

macro_rules! integers {
    ($($ty:ty),* $(,)?) => { $(impl Fingerprint for $ty {
        fn write_fingerprint(&self, hasher: &mut FingerprintHasher) { hasher.write(&self.to_le_bytes()); }
    })* };
}
integers!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);
impl Fingerprint for usize {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        (*self as u64).write_fingerprint(hasher);
    }
}
impl Fingerprint for isize {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        (*self as i64).write_fingerprint(hasher);
    }
}
impl Fingerprint for bool {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        u8::from(*self).write_fingerprint(hasher);
    }
}
impl Fingerprint for char {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        (*self as u32).write_fingerprint(hasher);
    }
}
impl Fingerprint for f32 {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        self.to_bits().write_fingerprint(hasher);
    }
}
impl Fingerprint for f64 {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        self.to_bits().write_fingerprint(hasher);
    }
}
impl Fingerprint for str {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        (self.len() as u64).write_fingerprint(hasher);
        hasher.write(self.as_bytes());
    }
}
impl Fingerprint for String {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        self.as_str().write_fingerprint(hasher);
    }
}
impl<T: Fingerprint> Fingerprint for [T] {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        (self.len() as u64).write_fingerprint(hasher);
        for item in self {
            item.write_fingerprint(hasher);
        }
    }
}
impl<T: Fingerprint> Fingerprint for Vec<T> {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        self.as_slice().write_fingerprint(hasher);
    }
}
impl<T: Fingerprint, const N: usize> Fingerprint for [T; N] {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        self.as_slice().write_fingerprint(hasher);
    }
}
impl<T: Fingerprint> Fingerprint for Option<T> {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        match self {
            None => 0_u8.write_fingerprint(hasher),
            Some(value) => {
                1_u8.write_fingerprint(hasher);
                value.write_fingerprint(hasher);
            }
        }
    }
}
impl<T: Fingerprint + ?Sized> Fingerprint for &T {
    fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
        (*self).write_fingerprint(hasher);
    }
}

macro_rules! tuples {
    ($(($($name:ident:$index:tt),+)),+ $(,)?) => {
        $(impl<$($name: Fingerprint),+> Fingerprint for ($($name,)+) {
            fn write_fingerprint(&self, hasher: &mut FingerprintHasher) {
                $(self.$index.write_fingerprint(hasher);)+
            }
        })+
    };
}
tuples!((A:0, B:1), (A:0, B:1, C:2), (A:0, B:1, C:2, D:3), (A:0, B:1, C:2, D:3, E:4), (A:0, B:1, C:2, D:3, E:4, F:5));
