//! Stream framing shared by both sides of every comparison (so a framing quirk cannot differ
//! between old and new): Annex-B access-unit splitters (the crates' own test splitters), IVF and Ogg.

/// Annex-B H.264 access units (crates/h264/tests/it/common/mod.rs `split_access_units`).
pub fn split_h264(data: &[u8]) -> Vec<&[u8]> {
    let starts = start_codes(data);
    let mut aus = Vec::new();
    let mut au_start = 0usize;
    let mut seen_slice = false;
    for &(sc, payload) in &starts {
        if payload >= data.len() {
            continue;
        }
        let t = data[payload] & 0x1f;
        let is_slice = t == 1 || t == 5;
        let first_mb_zero = is_slice && payload + 1 < data.len() && data[payload + 1] & 0x80 != 0;
        let boundary = seen_slice && (matches!(t, 6..=9) || first_mb_zero);
        if boundary {
            aus.push(&data[au_start..sc]);
            au_start = sc;
            seen_slice = false;
        }
        if is_slice {
            seen_slice = true;
        }
    }
    if au_start < data.len() {
        aus.push(&data[au_start..]);
    }
    aus
}

/// Annex-B H.265 access units (crates/hevc/tests/it/common/mod.rs `split_access_units`).
pub fn split_hevc(data: &[u8]) -> Vec<&[u8]> {
    let starts = start_codes(data);
    let mut aus = Vec::new();
    let mut au_start = 0usize;
    let mut seen_vcl = false;
    for &(sc, payload) in &starts {
        if payload + 2 >= data.len() {
            continue;
        }
        let t = (data[payload] >> 1) & 0x3f;
        let vcl = t < 32;
        let first_slice = vcl && data[payload + 2] & 0x80 != 0;
        let boundary = seen_vcl && (matches!(t, 32..=35 | 39) || first_slice);
        if boundary {
            aus.push(&data[au_start..sc]);
            au_start = sc;
            seen_vcl = false;
        }
        if vcl {
            seen_vcl = true;
        }
    }
    if au_start < data.len() {
        aus.push(&data[au_start..]);
    }
    aus
}

fn start_codes(data: &[u8]) -> Vec<(usize, usize)> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            let s = if i > 0 && data[i - 1] == 0 { i - 1 } else { i };
            starts.push((s, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
}

/// Frames of an IVF file: the header length from bytes 6..8, then 12-byte frame headers; a frame
/// that runs past the end is cut at the end of the file (as `examples/vp8dec.rs` does).
pub fn ivf_frames(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let Some(h) = data.get(6..8) else { return out };
    let mut pos = usize::from(u16::from_le_bytes([h[0], h[1]]));
    while pos + 12 <= data.len() {
        let size = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 12;
        let end = pos.saturating_add(size).min(data.len());
        out.push(&data[pos..end]);
        pos = end;
    }
    out
}

/// Minimal Ogg demuxer (crates/opus/tests/it/common/mod.rs `ogg_packets`), made safe on damaged input.
pub fn ogg_packets(data: &[u8]) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    let mut cur = Vec::new();
    let mut pos = 0;
    while pos + 27 <= data.len() && &data[pos..pos + 4] == b"OggS" {
        let nseg = data[pos + 26] as usize;
        let Some(table) = data.get(pos + 27..pos + 27 + nseg) else { break };
        let mut body = pos + 27 + nseg;
        for &l in table {
            let Some(seg) = data.get(body..body + l as usize) else { return packets };
            cur.extend_from_slice(seg);
            body += l as usize;
            if l < 255 {
                packets.push(std::mem::take(&mut cur));
            }
        }
        pos = body;
    }
    packets
}

/// An RFC 6716 `.bit` test vector: repeated `u32be length, u32be final range, packet`.
pub fn bit_packets(data: &[u8]) -> Vec<(Vec<u8>, u32)> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 8 <= data.len() {
        let len = u32::from_be_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]) as usize;
        let rng = u32::from_be_bytes([data[p + 4], data[p + 5], data[p + 6], data[p + 7]]);
        p += 8;
        if p.saturating_add(len) > data.len() {
            break;
        }
        out.push((data[p..p + len].to_vec(), rng));
        p += len;
    }
    out
}
