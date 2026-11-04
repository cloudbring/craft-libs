//! Hand-built H.265 parameter sets and I-slice headers with extreme QP fields, plus hvcC records.
//! Adapted from `crates/hevc/src/synth_tests.rs` (VPS/SPS/PPS writers); slice data is junk, because
//! the QP bounds the #48 hardening commit adds are checked in the headers, before any slice data.

use craft_bitstream::{BitWriter, escape_rbsp};

const W: u32 = 96;
const H: u32 = 64;

fn nal(t: u8, rbsp: &[u8]) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 1, t << 1, 1];
    out.extend(escape_rbsp(rbsp));
    out
}

fn ptl(w: &mut BitWriter) {
    w.write_bits(0, 2);
    w.write_bits(0, 1);
    w.write_bits(1, 5);
    w.write_bits(0x6000_0000, 32);
    w.write_bits(0b1001, 4);
    w.write_bits(0, 32);
    w.write_bits(0, 11);
    w.write_bits(0, 1);
    w.write_bits(93, 8);
}

pub fn vps() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_bits(0, 4);
    w.write_bits(3, 2);
    w.write_bits(0, 6);
    w.write_bits(0, 3);
    w.write_bits(1, 1);
    w.write_bits(0xffff, 16);
    ptl(&mut w);
    w.write_bits(1, 1);
    w.write_ue(4);
    w.write_ue(0);
    w.write_ue(0);
    w.write_bits(0, 6);
    w.write_ue(0);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.rbsp_trailing();
    nal(32, &w.finish())
}

pub fn sps() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_bits(0, 4);
    w.write_bits(0, 3);
    w.write_bits(1, 1);
    ptl(&mut w);
    w.write_ue(0);
    w.write_ue(1);
    w.write_ue(W);
    w.write_ue(H);
    w.write_bits(0, 1);
    w.write_ue(0);
    w.write_ue(0);
    w.write_ue(4);
    w.write_bits(1, 1);
    w.write_ue(4);
    w.write_ue(0);
    w.write_ue(0);
    w.write_ue(0);
    w.write_ue(1);
    w.write_ue(0);
    w.write_ue(2);
    w.write_ue(0);
    w.write_ue(0);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(1, 1);
    w.write_bits(7, 4);
    w.write_bits(7, 4);
    w.write_ue(0);
    w.write_ue(1);
    w.write_bits(0, 1);
    w.write_ue(0);
    w.write_bits(1, 1);
    w.write_ue(0);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.rbsp_trailing();
    nal(33, &w.finish())
}

pub fn pps(init_qp: i32, cb: i32, cr: i32, cu_qp_delta: bool) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_ue(0);
    w.write_ue(0);
    w.write_bits(1, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 3);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_ue(0);
    w.write_ue(0);
    w.write_se(init_qp);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(cu_qp_delta as u32, 1);
    if cu_qp_delta {
        w.write_ue(0);
    }
    w.write_se(cb);
    w.write_se(cr);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(1, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.write_ue(0);
    w.write_bits(0, 1);
    w.write_bits(0, 1);
    w.rbsp_trailing();
    nal(34, &w.finish())
}

/// An IDR_W_RADL I-slice with the given `slice_qp_delta` and junk slice data.
pub fn idr_slice(qp_delta: i32, junk: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_bits(1, 1); // first_slice_segment_in_pic_flag
    w.write_bits(0, 1); // no_output_of_prior_pics_flag
    w.write_ue(0); // pps id
    w.write_ue(2); // slice_type I
    w.write_se(qp_delta);
    w.write_bits(1, 1); // slice_loop_filter_across_slices_enabled_flag
    w.write_bits(1, 1); // byte_alignment
    w.align_zero();
    w.write_bytes(junk);
    w.rbsp_trailing();
    nal(19, &w.finish())
}

const QPS: [i32; 16] = [0, 1, -1, 25, -25, 26, -26, 51, -51, -75, 100, 1 << 20, i32::MAX - 1, i32::MIN + 2, i32::MAX, i32::MIN + 1];

pub fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let junk = [0x80u8, 0x00, 0x12, 0x34, 0x56, 0x78];
    for &q in &QPS {
        let mut v = vps();
        v.extend(sps());
        v.extend(pps(q, 0, 0, false));
        v.extend(idr_slice(0, &junk));
        out.push((format!("pps-initqp-{q}"), v));
        for cu in [false, true] {
            let mut v = vps();
            v.extend(sps());
            v.extend(pps(0, q, 0, cu));
            v.extend(idr_slice(0, &junk));
            out.push((format!("pps-cb-{q}-cu{}", cu as u8), v));
            let mut v = vps();
            v.extend(sps());
            v.extend(pps(0, 0, q, cu));
            v.extend(idr_slice(0, &junk));
            out.push((format!("pps-cr-{q}-cu{}", cu as u8), v));
        }
        for init in [0, 26, -26, 25] {
            let mut v = vps();
            v.extend(sps());
            v.extend(pps(init, 0, 0, false));
            v.extend(idr_slice(q, &junk));
            out.push((format!("slice-qp-i{init}-{q}"), v));
        }
    }
    out
}

/// hvcC with the VPS, SPS and PPS arrays (and optionally the slice), 23 bytes of header.
pub fn hvcc(include_slice: bool) -> (Vec<u8>, Vec<u8>) {
    let mut stream = vps();
    stream.extend(sps());
    stream.extend(pps(0, 0, 0, false));
    let slice = idr_slice(0, &[0x80, 0, 1, 2, 3, 4]);
    let mut nals: Vec<&[u8]> = craft_bitstream::annexb_nals(&stream);
    let slice_nals = craft_bitstream::annexb_nals(&slice);
    let all: Vec<Vec<u8>> = nals.iter().map(|n| n.to_vec()).collect();
    if include_slice {
        nals.extend(slice_nals);
    }
    let mut config = vec![0; 23];
    config[0] = 1;
    config[21] = 3; // lengthSizeMinusOne
    config[22] = nals.len() as u8;
    for nal in &nals {
        config.extend_from_slice(&[nal[0] >> 1, 0, 1]);
        config.extend_from_slice(&(nal.len() as u16).to_be_bytes());
        config.extend_from_slice(nal);
    }
    let _ = all;
    (config, slice)
}

/// hvcC probe inputs: `[u32 BE hvcC length][hvcC][Annex-B slice stream]`.
pub fn hvcc_corpus() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for include_slice in [false, true] {
        let (valid, slice) = hvcc(include_slice);
        let pack = |a: &[u8]| {
            let mut v = (a.len() as u32).to_be_bytes().to_vec();
            v.extend_from_slice(a);
            v.extend_from_slice(&slice);
            v
        };
        let tag = if include_slice { "with-slice" } else { "ps-only" };
        out.push((format!("hvcc-{tag}-valid"), pack(&valid)));
        for pos in 0..valid.len().min(80) {
            for bit in 0..8 {
                let mut a = valid.clone();
                a[pos] ^= 1 << bit;
                out.push((format!("hvcc-{tag}-flip@{pos}.{bit}"), pack(&a)));
            }
            for v in [0u8, 0xff] {
                let mut a = valid.clone();
                a[pos] = v;
                out.push((format!("hvcc-{tag}-set{v:02x}@{pos}"), pack(&a)));
            }
        }
        for cut in (0..valid.len()).step_by(3) {
            out.push((format!("hvcc-{tag}-trunc{cut}"), pack(&valid[..cut])));
        }
    }
    out
}
