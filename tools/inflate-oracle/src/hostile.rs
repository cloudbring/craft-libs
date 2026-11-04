//! Deterministic hostile DEFLATE streams. Every case is `(name, bytes)`; nothing is written to disk.

/// LSB-first bit writer (DEFLATE bit order). Huffman codes are packed MSB-first via `huff`.
#[derive(Default)]
pub struct Bits {
    pub out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    pub fn put(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    pub fn huff(&mut self, code: u32, len: u32) {
        for i in (0..len).rev() {
            self.put((code >> i) & 1, 1);
        }
    }
    pub fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
    /// Fixed-Huffman literal/length symbol.
    pub fn fixed_sym(&mut self, sym: u32) {
        match sym {
            0..=143 => self.huff(0x30 + sym, 8),
            144..=255 => self.huff(0x190 + sym - 144, 9),
            256..=279 => self.huff(sym - 256, 7),
            _ => self.huff(0xC0 + sym - 280, 8),
        }
    }
}

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn compress(data: &[u8], level: u32) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(level));
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn sample(len: usize, seed: u64) -> Vec<u8> {
    let mut r = Rng(seed | 1);
    let words: [&[u8]; 6] = [b"the ", b"deflate ", b"stream ", b"<w:p>", b"0123456789", b"\x00\x00\x00\x00"];
    let mut v = Vec::new();
    while v.len() < len {
        if r.below(5) == 0 {
            v.push(r.next() as u8);
        } else {
            v.extend_from_slice(words[r.below(words.len())]);
        }
    }
    v.truncate(len);
    v
}

/// Fixed block: BFINAL=1 BTYPE=01, then `body`, then end-of-block.
fn fixed(body: impl FnOnce(&mut Bits)) -> Vec<u8> {
    let mut b = Bits::default();
    b.put(1, 1);
    b.put(1, 2);
    body(&mut b);
    b.fixed_sym(256);
    b.finish()
}

/// Dynamic block header with a code-length alphabet where symbols 0 and 1 have length 1 and every
/// other code-length symbol is unused; the caller then writes each literal/length and distance code
/// length as a single bit (0 -> sym 0 -> length 0, 1 -> sym 1 -> length 1).
fn dynamic_header(b: &mut Bits, hlit: u32, hdist: u32) {
    b.put(1, 1);
    b.put(2, 2);
    b.put(hlit, 5);
    b.put(hdist, 5);
    b.put(14, 4); // HCLEN: 18 entries in the order 16 17 18 0 8 7 9 6 10 5 11 4 12 3 13 2 14 1 15..
    for i in 0..18u32 {
        // position 3 holds symbol 0, position 17 holds symbol 1
        b.put(if i == 3 || i == 17 { 1 } else { 0 }, 3);
    }
}

pub fn cases() -> Vec<(String, Vec<u8>)> {
    let mut c: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |n: &str, v: Vec<u8>| c.push((format!("hostile/{n}"), v));

    // Controls that are valid: all decoders must agree on the bytes.
    for (lvl, len) in [(0u32, 300usize), (1, 5000), (6, 40_000), (9, 70_000)] {
        add(&format!("valid/level{lvl}-{len}"), compress(&sample(len, 7 + len as u64), lvl));
    }
    add("valid/empty-fixed", fixed(|_| {}));
    add(
        "valid/len258-dist1",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(285);
            b.huff(0, 5); // distance symbol 0 = distance 1
        }),
    );
    let mut stored = vec![1u8, 5, 0, 0xFA, 0xFF];
    stored.extend_from_slice(b"hello");
    add("valid/stored", stored.clone());
    let mut trailing = stored.clone();
    trailing.extend_from_slice(b"TRAILING-GARBAGE");
    add("edge/trailing-after-final", trailing);
    add("edge/empty-input", vec![]);
    add("edge/one-byte-zero", vec![0]);

    // Truncations of valid streams at several cut points (every byte for short ones).
    let base = compress(&sample(3000, 11), 6);
    for cut in 0..base.len().min(64) {
        add(&format!("truncated/short-{cut:03}"), base[..cut].to_vec());
    }
    for k in 1..16 {
        let cut = base.len() * k / 16;
        add(&format!("truncated/frac-{k:02}"), base[..cut].to_vec());
    }
    add("truncated/minus-one", base[..base.len() - 1].to_vec());
    let mut stored_trunc = stored.clone();
    stored_trunc.truncate(4);
    add("truncated/stored-in-header", stored_trunc);
    add("truncated/stored-payload", stored[..8].to_vec());

    // Reserved block type 11.
    add("blocktype/11-final", vec![0x07, 0, 0, 0, 0]);
    add("blocktype/11-nonfinal", vec![0x06, 0, 0, 0, 0]);
    let mut late = fixed(|b| b.fixed_sym(u32::from(b'x')));
    late[0] &= !1; // not final
    late.push(0x06); // then a block type 11
    late.push(0);
    add("blocktype/11-after-valid-block", late);

    // Stored-block LEN/NLEN mismatch and short payload.
    add("stored/nlen-not-complement", vec![1, 5, 0, 0xFA, 0xFE, b'h', b'e', b'l', b'l', b'o']);
    add("stored/nlen-zero", vec![1, 5, 0, 0, 0, b'h', b'e', b'l', b'l', b'o']);
    add("stored/len-exceeds-input", vec![1, 0x10, 0, 0xEF, 0xFF, b'h', b'i']);
    add("stored/len-ffff-no-payload", vec![1, 0xFF, 0xFF, 0, 0]);

    // Oversubscribed Huffman codes.
    for (hlit, hdist, nlit, ndist, nm) in [(0u32, 0u32, 257u32, 1u32, "litlen-257-of-len1"), (0, 1, 257, 2, "all-of-len1-with-dist")] {
        let mut b = Bits::default();
        dynamic_header(&mut b, hlit, hdist);
        for _ in 0..nlit {
            b.huff(1, 1);
        }
        for _ in 0..ndist {
            b.huff(1, 1);
        }
        b.put(0, 8);
        add(&format!("oversubscribed/{nm}"), b.finish());
    }
    // Code-length code itself oversubscribed: 19 symbols of length 1.
    {
        let mut b = Bits::default();
        b.put(1, 1);
        b.put(2, 2);
        b.put(0, 5);
        b.put(0, 5);
        b.put(15, 4);
        for _ in 0..19 {
            b.put(1, 3);
        }
        b.put(0, 16);
        add("oversubscribed/code-length-code", b.finish());
    }
    // Incomplete (under-subscribed) literal code: one symbol of length 1 and nothing else.
    {
        let mut b = Bits::default();
        dynamic_header(&mut b, 0, 0);
        b.huff(1, 1); // sym 0 gets length 1... shifted below
        for _ in 1..256 {
            b.huff(0, 1);
        }
        b.huff(1, 1); // end-of-block gets length 1; only two codes, complete
        b.huff(1, 1); // distance code of length 1 (single code, allowed)
        b.put(0, 1); // literal 0
        b.put(1, 1); // end of block
        add("incomplete/two-lit-codes-complete", b.finish());
        let mut b = Bits::default();
        dynamic_header(&mut b, 0, 0);
        for _ in 0..256 {
            b.huff(0, 1);
        }
        b.huff(1, 1); // only end-of-block, length 1: incomplete (half the code space)
        b.huff(1, 1);
        b.put(0, 1); // the end-of-block code
        add("incomplete/single-eob-len1", b.finish());
        let mut b = Bits::default();
        dynamic_header(&mut b, 0, 0);
        for _ in 0..257 {
            b.huff(0, 1); // no codes at all
        }
        b.huff(0, 1);
        b.put(0, 8);
        add("incomplete/empty-litlen", b.finish());
    }
    // Dynamic block without an end-of-block code (length 0 for symbol 256).
    {
        let mut b = Bits::default();
        dynamic_header(&mut b, 0, 0);
        b.huff(1, 1);
        b.huff(1, 1);
        for _ in 2..257 {
            b.huff(0, 1);
        }
        b.huff(1, 1);
        b.put(0, 8);
        add("dynamic/no-eob-code", b.finish());
    }

    // Distance too far back.
    add(
        "distance/before-start-empty-window",
        fixed(|b| {
            b.fixed_sym(257); // length 3
            b.huff(0, 5); // distance 1 with nothing written yet
        }),
    );
    add(
        "distance/dist5-after-one-literal",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(257);
            b.huff(4, 5); // distance symbol 4 = 5..6
            b.put(0, 1);
        }),
    );
    add(
        "distance/max-32768-after-few-literals",
        fixed(|b| {
            for _ in 0..10 {
                b.fixed_sym(u32::from(b'z'));
            }
            b.fixed_sym(257);
            b.huff(29, 5); // distance symbol 29 = 24577..32768, 13 extra bits
            b.put(0x1FFF, 13);
        }),
    );
    add(
        "distance/reserved-symbol-30",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(257);
            b.huff(30, 5);
        }),
    );
    add(
        "distance/reserved-symbol-31",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(257);
            b.huff(31, 5);
        }),
    );

    // Length out of range / reserved.
    add(
        "length/reserved-symbol-286",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(286);
            b.huff(0, 5);
        }),
    );
    add(
        "length/reserved-symbol-287",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(287);
            b.huff(0, 5);
        }),
    );
    // Symbol 284 with all five extra bits set encodes 258; RFC 1951 gives 258 only to symbol 285.
    add(
        "length/284-extra-31-is-258",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(284);
            b.put(31, 5);
            b.huff(0, 5);
        }),
    );
    add(
        "length/285-then-extra-bits-ignored",
        fixed(|b| {
            b.fixed_sym(u32::from(b'a'));
            b.fixed_sym(285);
            b.put(0, 5); // stray: 285 has no extra bits, this reads as the next symbol's prefix
            b.huff(0, 5);
        }),
    );

    // Code-length repeat misuse in dynamic headers.
    {
        // Needs symbol 16 in the CL alphabet: give 16 and 1 length 1.
        let mut b = Bits::default();
        b.put(1, 1);
        b.put(2, 2);
        b.put(0, 5);
        b.put(0, 5);
        b.put(14, 4);
        for i in 0..18u32 {
            b.put(if i == 0 || i == 17 { 1 } else { 0 }, 3);
        } // 16 -> code 0, 1 -> code 1
        b.huff(0, 1); // repeat previous with no previous
        b.put(0, 2);
        b.put(0, 16);
        add("clrepeat/16-without-previous", b.finish());
    }
    c
}

/// Seeded single-byte and bit mutations of valid streams: `count` cases.
pub fn mutations(count: usize, seed: u64) -> Vec<(String, Vec<u8>)> {
    let bases: Vec<Vec<u8>> = (0..6).map(|i| compress(&sample(800 + i * 700, 100 + i as u64), [1, 3, 6, 9, 6, 2][i])).collect();
    let mut r = Rng(seed);
    let mut out = Vec::new();
    for i in 0..count {
        let mut v = bases[r.below(bases.len())].clone();
        match r.below(4) {
            0 => {
                let p = r.below(v.len());
                v[p] ^= 1 << r.below(8);
            }
            1 => {
                let p = r.below(v.len());
                v[p] = r.next() as u8;
            }
            2 => {
                let p = r.below(v.len().min(40));
                v[p] ^= 1 << r.below(8);
            } // header region
            _ => {
                let p = r.below(v.len());
                let q = r.below(v.len());
                v[p] ^= 1 << r.below(8);
                v[q] ^= 1 << r.below(8);
            }
        }
        out.push((format!("mutation/{i:05}"), v));
    }
    out
}

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0;
        for i in 0..n {
            let byte = *self.d.get(self.pos / 8)?;
            v |= u32::from((byte >> (self.pos % 8)) & 1) << i;
            self.pos += 1;
        }
        Some(v)
    }
}

/// Independent classifier for the one difference class the oracle registers: does the first block
/// of `stream` carry a dynamic header whose literal/length code is incomplete (Kraft sum < 1) with
/// a longest code over one bit? zlib and miniz_oxide reject that at table build ("invalid
/// literal/lengths set"); lean-zip's specification only requires a prefix-free code, so it accepts
/// the header and fails later only if a stream actually reaches an unassigned code.
pub fn first_block_incomplete_litlen(stream: &[u8]) -> bool {
    first_block_incomplete_litlen_inner(stream).unwrap_or(false)
}

fn first_block_incomplete_litlen_inner(stream: &[u8]) -> Option<bool> {
    let mut r = Reader { d: stream, pos: 0 };
    let _final = r.bits(1)?;
    if r.bits(2)? != 2 {
        return Some(false);
    }
    let hlit = r.bits(5)? as usize + 257;
    let hdist = r.bits(5)? as usize + 1;
    let hclen = r.bits(4)? as usize + 4;
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let mut cl = [0u32; 19];
    for &o in ORDER.iter().take(hclen) {
        cl[o] = r.bits(3)?;
    }
    // Canonical code-length code.
    let mut count = [0u32; 8];
    for &l in &cl {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 9];
    let mut code = 0;
    for l in 1..8 {
        code = (code + count[l - 1]) << 1;
        next[l] = code;
    }
    let mut table = Vec::new(); // (len, code, symbol)
    for (sym, &l) in cl.iter().enumerate() {
        if l > 0 {
            table.push((l, next[l as usize], sym));
            next[l as usize] += 1;
        }
    }
    let mut lens: Vec<u32> = Vec::new();
    while lens.len() < hlit + hdist {
        let (mut code, mut len) = (0u32, 0u32);
        let sym = loop {
            code = (code << 1) | r.bits(1)?;
            len += 1;
            if let Some(&(_, _, s)) = table.iter().find(|&&(l, c, _)| l == len && c == code) {
                break s;
            }
            if len > 7 {
                return Some(false);
            }
        };
        match sym {
            0..=15 => lens.push(sym as u32),
            16 => {
                let prev = *lens.last()?;
                let n = 3 + r.bits(2)?;
                lens.extend(std::iter::repeat_n(prev, n as usize));
            }
            17 => {
                let n = 3 + r.bits(3)?;
                lens.extend(std::iter::repeat_n(0, n as usize));
            }
            _ => {
                let n = 11 + r.bits(7)?;
                lens.extend(std::iter::repeat_n(0, n as usize));
            }
        }
    }
    if lens.len() != hlit + hdist {
        return Some(false);
    }
    let lit = &lens[..hlit];
    let max = lit.iter().copied().max().unwrap_or(0);
    let kraft: u64 = lit.iter().filter(|&&l| l > 0).map(|&l| 1u64 << (15 - l)).sum();
    Some(max > 1 && kraft < (1 << 15))
}
