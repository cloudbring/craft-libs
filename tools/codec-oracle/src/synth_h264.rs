//! Hand-built H.264 streams (I_PCM and P_Skip macroblocks only, CAVLC) with extreme header
//! fields. The builder is adapted from `crates/h264/src/synth_tests.rs` (private to that crate):
//! SPS/PPS/slice-header writers over `craft_bitstream::BitWriter`. Where the crate's tests build
//! one stream per fix, this generates grids of values, because the #48 hardening commits bound
//! fields (slice QP, frame_num gaps, POC offsets, reference list modification, MMCO operands,
//! skip runs, avcC records) that real encoders never push to their limits.

use craft_bitstream::{BitWriter, escape_rbsp};

const MB_W: usize = 4;
const MB_H: usize = 3;
pub const NMB: usize = MB_W * MB_H;

fn nal(ref_idc: u8, t: u8, rbsp: &[u8]) -> Vec<u8> {
    let mut v = vec![0, 0, 0, 1, (ref_idc << 5) | t];
    v.extend(escape_rbsp(rbsp));
    v
}

#[derive(Clone)]
pub enum Poc {
    /// `log2_max_pic_order_cnt_lsb` = bits.
    T0(u32),
    T1 {
        non_ref: i32,
        top_bottom: i32,
        cycle: Vec<i32>,
    },
    T2,
}

#[derive(Clone)]
pub struct Seq {
    pub max_refs: u32,
    pub gaps: bool,
    pub frame_bits: u32,
    pub poc: Poc,
}

impl Seq {
    pub fn new(poc: Poc) -> Self {
        Seq { max_refs: 4, gaps: false, frame_bits: 4, poc }
    }
    pub fn sps(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.write_bits(66, 8);
        w.write_bits(0, 8);
        w.write_bits(30, 8);
        w.write_ue(0);
        w.write_ue(self.frame_bits - 4);
        match &self.poc {
            Poc::T0(bits) => {
                w.write_ue(0);
                w.write_ue(bits - 4);
            }
            Poc::T1 { non_ref, top_bottom, cycle } => {
                w.write_ue(1);
                w.write_bit(false);
                w.write_se(*non_ref);
                w.write_se(*top_bottom);
                w.write_ue(cycle.len() as u32);
                for c in cycle {
                    w.write_se(*c);
                }
            }
            Poc::T2 => w.write_ue(2),
        }
        w.write_ue(self.max_refs);
        w.write_bit(self.gaps);
        w.write_ue(MB_W as u32 - 1);
        w.write_ue(MB_H as u32 - 1);
        w.write_bit(true);
        w.write_bit(true);
        w.write_bit(false);
        w.write_bit(false);
        w.rbsp_trailing();
        nal(3, 7, &w.finish())
    }
    pub fn pps(&self) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.write_ue(0);
        w.write_ue(0);
        w.write_bit(false);
        w.write_bit(false);
        w.write_ue(0);
        w.write_ue(0);
        w.write_ue(0);
        w.write_bit(false);
        w.write_bits(0, 2);
        w.write_se(0);
        w.write_se(0);
        w.write_se(0);
        w.write_bit(true);
        w.write_bit(false);
        w.write_bit(false);
        w.rbsp_trailing();
        nal(3, 8, &w.finish())
    }
}

#[derive(Default, Clone)]
pub struct Pic {
    pub idr: bool,
    pub frame_num: u32,
    pub ref_idc: u8,
    pub mods: Vec<(u32, u32)>,
    pub mmcos: Vec<(u32, u32, u32)>,
    /// Some(samples) = I_PCM, None = P_Skip.
    pub mbs: Vec<Option<Vec<u8>>>,
    pub poc_lsb: Option<(u32, u32)>,
    pub poc_delta: Option<i32>,
    pub qp_delta: i32,
    /// Replace the trailing mb_skip_run with this value.
    pub skip_run: Option<u32>,
}

impl Pic {
    fn is_p(&self) -> bool {
        !self.idr && self.mbs.iter().any(|m| m.is_none())
    }
    pub fn nal(&self, frame_bits: u32) -> Vec<u8> {
        let mut w = BitWriter::new();
        w.write_ue(0);
        w.write_ue(if self.is_p() { 5 } else { 7 });
        w.write_ue(0);
        w.write_bits(self.frame_num & ((1u64 << frame_bits) as u32).wrapping_sub(1), frame_bits);
        if self.idr {
            w.write_ue(0);
        }
        if let Some((lsb, bits)) = self.poc_lsb {
            w.write_bits(lsb & (((1u64 << bits) - 1) as u32), bits);
        }
        if let Some(d) = self.poc_delta {
            w.write_se(d);
        }
        if self.is_p() {
            w.write_bit(false);
            w.write_bit(!self.mods.is_empty());
            if !self.mods.is_empty() {
                for &(idc, v) in &self.mods {
                    w.write_ue(idc);
                    w.write_ue(v);
                }
                w.write_ue(3);
            }
        }
        if self.ref_idc != 0 {
            if self.idr {
                w.write_bit(false);
                w.write_bit(false);
            } else {
                w.write_bit(!self.mmcos.is_empty());
                if !self.mmcos.is_empty() {
                    for &(op, a, b) in &self.mmcos {
                        w.write_ue(op);
                        match op {
                            1 | 2 | 4 => w.write_ue(a),
                            3 => {
                                w.write_ue(a);
                                w.write_ue(b);
                            }
                            6 => w.write_ue(b),
                            _ => {}
                        }
                    }
                    w.write_ue(0);
                }
            }
        }
        w.write_se(self.qp_delta);
        w.write_ue(0);
        w.write_se(0);
        w.write_se(0);
        let mut run = 0u32;
        for mb in &self.mbs {
            match mb {
                None => run += 1,
                Some(s) => {
                    if self.is_p() {
                        w.write_ue(run);
                        run = 0;
                        w.write_ue(30);
                    } else {
                        w.write_ue(25);
                    }
                    w.align_zero();
                    w.write_bytes(s);
                }
            }
        }
        if let Some(r) = self.skip_run {
            w.write_ue(r);
        } else if run > 0 {
            w.write_ue(run);
        }
        w.rbsp_trailing();
        nal(self.ref_idc, if self.idr { 5 } else { 1 }, &w.finish())
    }
}

fn mb(seed: &mut u32) -> Vec<u8> {
    (0..384)
        .map(|_| {
            *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (*seed >> 24) as u8
        })
        .collect()
}

pub fn idr(seed: &mut u32, qp_delta: i32, seq: &Seq) -> Pic {
    Pic {
        idr: true,
        frame_num: 0,
        ref_idc: 3,
        mbs: (0..NMB).map(|_| Some(mb(seed))).collect(),
        qp_delta,
        poc_lsb: if let Poc::T0(b) = seq.poc { Some((0, b)) } else { None },
        poc_delta: if let Poc::T1 { .. } = seq.poc { Some(0) } else { None },
        ..Default::default()
    }
}

pub fn skip_p(frame_num: u32, ref_idc: u8, seq: &Seq) -> Pic {
    Pic {
        frame_num,
        ref_idc,
        mbs: vec![None; NMB],
        poc_lsb: if let Poc::T0(b) = seq.poc { Some((frame_num * 2, b)) } else { None },
        poc_delta: if let Poc::T1 { .. } = seq.poc { Some(0) } else { None },
        ..Default::default()
    }
}

pub fn stream(seq: &Seq, pics: &[Pic]) -> Vec<u8> {
    let mut v = seq.sps();
    v.extend(seq.pps());
    for p in pics {
        v.extend(p.nal(seq.frame_bits));
    }
    v
}

const BIG: [u32; 8] = [0, 1, 15, 16, 65535, i32::MAX as u32, u32::MAX - 1, 1 << 24];
const QPS: [i32; 14] = [0, 1, -1, 25, 26, -26, 27, -27, 100, -100, 1 << 20, i32::MAX - 1, i32::MIN + 2, i32::MAX];

/// All synthetic streams, `(name, bytes)`.
pub fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut seed = 12345u32;
    // slice QP deltas, in the IDR and in a following P frame
    for poc in [Poc::T0(4), Poc::T2] {
        let pn = if matches!(poc, Poc::T2) { "t2" } else { "t0" };
        let seq = Seq::new(poc);
        for &q in &QPS {
            let pics = [idr(&mut seed, q, &seq), skip_p(1, 2, &seq)];
            out.push((format!("qp-idr-{pn}-{q}"), stream(&seq, &pics)));
            let mut p1 = skip_p(1, 2, &seq);
            p1.qp_delta = q;
            let pics = [idr(&mut seed, 0, &seq), p1];
            out.push((format!("qp-p-{pn}-{q}"), stream(&seq, &pics)));
        }
    }
    // frame_num gaps
    for frame_bits in [4u32, 6, 8, 16] {
        for gaps in [true, false] {
            for max_refs in [1u32, 2, 4, 16] {
                for jump in [2u32, 3, 5, 20, 40, 255, 1000, 65535] {
                    if frame_bits < 16 && jump >= (1 << frame_bits) {
                        continue;
                    }
                    let mut seq = Seq::new(Poc::T2);
                    seq.frame_bits = frame_bits;
                    seq.gaps = gaps;
                    seq.max_refs = max_refs;
                    let pics = [idr(&mut seed, 0, &seq), skip_p(jump, 2, &seq), skip_p(jump + 1, 2, &seq)];
                    out.push((format!("gap-b{frame_bits}-g{}-r{max_refs}-j{jump}", gaps as u8), stream(&seq, &pics)));
                }
            }
        }
    }
    // POC: type 1 with extreme offsets and deltas, type 0 with long lsb, frame_num wrap
    let ext = [0i32, 4, -3, i32::MAX, i32::MIN + 1, i32::MAX / 2];
    for &non_ref in &[0i32, -3, i32::MAX, i32::MIN + 1] {
        for &tb in &[0i32, i32::MAX, i32::MIN + 1] {
            for cyc in [vec![4], vec![i32::MAX], vec![i32::MAX, i32::MAX], vec![i32::MIN + 1, 7, i32::MAX]] {
                for &d in &ext {
                    let mut seq = Seq::new(Poc::T1 { non_ref, top_bottom: tb, cycle: cyc.clone() });
                    seq.frame_bits = 8;
                    seq.gaps = true;
                    let mut pics = vec![idr(&mut seed, 0, &seq)];
                    for (i, fnum) in [1u32, 2, 3, 5, 9, 200, 201].iter().enumerate() {
                        let mut p = skip_p(*fnum, if i % 2 == 0 { 2 } else { 0 }, &seq);
                        p.poc_delta = Some(d);
                        pics.push(p);
                    }
                    out.push((format!("poc1-n{non_ref}-t{tb}-c{}-d{d}", cyc.len()), stream(&seq, &pics)));
                }
            }
        }
    }
    for bits in [4u32, 8, 16] {
        let seq = Seq::new(Poc::T0(bits));
        for &lsb in &[0u32, 1, 7, 15, 255, 65535, u32::MAX] {
            let mut pics = vec![idr(&mut seed, 0, &seq)];
            for f in 1..6u32 {
                let mut p = skip_p(f, 2, &seq);
                p.poc_lsb = Some((lsb.wrapping_mul(f), bits));
                pics.push(p);
            }
            out.push((format!("poc0-b{bits}-l{lsb}"), stream(&seq, &pics)));
        }
    }
    // reference list modification
    let seq = Seq::new(Poc::T2);
    for idc in [0u32, 1, 2, 3, 4, 7] {
        for &v in &BIG {
            for n in [1usize, 3] {
                let mut p = skip_p(1, 2, &seq);
                p.mods = vec![(idc, v); n];
                let pics = [idr(&mut seed, 0, &seq), p];
                out.push((format!("mod-i{idc}-v{v}-n{n}"), stream(&seq, &pics)));
            }
        }
    }
    // MMCO operands
    for op in 1u32..=7 {
        for &a in &BIG {
            for &b in &[0u32, 1, 16, i32::MAX as u32] {
                let mut p1 = skip_p(1, 2, &seq);
                p1.mmcos = vec![(op, a, b)];
                let pics = [idr(&mut seed, 0, &seq), p1, skip_p(2, 2, &seq)];
                out.push((format!("mmco-o{op}-a{a}-b{b}"), stream(&seq, &pics)));
            }
        }
    }
    // mb_skip_run
    for &r in &[0u32, 1, 11, 12, 13, 100, 65535, i32::MAX as u32, u32::MAX - 1] {
        let mut p = skip_p(1, 2, &seq);
        p.skip_run = Some(r);
        let pics = [idr(&mut seed, 0, &seq), p];
        out.push((format!("skiprun-{r}"), stream(&seq, &pics)));
    }
    out
}

/// A valid avcC record for the synthetic sequence (4-byte NAL lengths, one SPS, one PPS).
pub fn avcc(seq: &Seq) -> Vec<u8> {
    let (sps, pps) = (seq.sps(), seq.pps());
    let mut v = vec![1, 66, 0, 30, 0xff, 0xe1];
    v.extend_from_slice(&((sps.len() - 4) as u16).to_be_bytes());
    v.extend_from_slice(&sps[4..]);
    v.push(1);
    v.extend_from_slice(&((pps.len() - 4) as u16).to_be_bytes());
    v.extend_from_slice(&pps[4..]);
    v
}

/// avcC probe inputs: `[u32 BE avcc length][avcC][Annex-B stream]`; the stream decodes after the record is configured.
pub fn avcc_corpus() -> Vec<(String, Vec<u8>)> {
    let mut seed = 7u32;
    let seq = Seq::new(Poc::T2);
    let stream = stream(&seq, &[idr(&mut seed, 0, &seq), skip_p(1, 2, &seq)]);
    let valid = avcc(&seq);
    let pack = |a: &[u8]| {
        let mut v = (a.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(a);
        v.extend_from_slice(&stream);
        v
    };
    let mut out = vec![("avcc-valid".to_string(), pack(&valid))];
    for pos in 0..valid.len() {
        for bit in 0..8 {
            let mut a = valid.clone();
            a[pos] ^= 1 << bit;
            out.push((format!("avcc-flip@{pos}.{bit}"), pack(&a)));
        }
        for v in [0u8, 0xff] {
            let mut a = valid.clone();
            a[pos] = v;
            out.push((format!("avcc-set{v:02x}@{pos}"), pack(&a)));
        }
    }
    for cut in 0..valid.len() {
        out.push((format!("avcc-trunc{cut}"), pack(&valid[..cut])));
    }
    // type swap as in the crate's regression: the SPS NAL byte replaced by the PPS NAL byte
    let mut swapped = valid.clone();
    swapped[8] = seq.pps()[4];
    out.push(("avcc-type-swap".into(), pack(&swapped)));
    out
}
