//! One decoder driver per codec, instantiated once per implementation (old fork or new crate).
//! The driver is the same text for both sides, so any difference is the decoder's.

use crate::containers::*;
use crate::trace::*;

/// `[u64 BE memory budget][stream]`: decode with `set_memory_budget` applied (single thread). Only
/// deckcraft's forks and the shared crates have the setter; filmcraft and soundcraft never had a budget.
macro_rules! budget_driver {
    ($m:ident, $codec:ident, $name:ident, $split:expr, $kind:ident) => {
        pub fn $name(bytes: &[u8]) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let Some(b) = bytes.get(..8).map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])) else { return t };
                let mut dec = $crate::sides::api::$m::$codec::Decoder::with_threads(1);
                dec.set_memory_budget(b);
                for (i, au) in $split(&bytes[8..]).into_iter().enumerate() {
                    t.units += 1;
                    match budget_decode!($kind, dec, au, i) {
                        Ok(pics) => {
                            for p in &pics {
                                budget_push!($kind, t, p);
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                for p in &dec.flush() {
                    budget_push!($kind, t, p);
                }
                t
            })
        }
    };
}
macro_rules! budget_decode {
    (annexb, $dec:ident, $au:ident, $i:ident) => {
        $dec.decode($au, $i as i64)
    };
    (av1, $dec:ident, $au:ident, $i:ident) => {
        $dec.decode_pts($au, $i as i64)
    };
    (vp9, $dec:ident, $au:ident, $i:ident) => {
        $dec.decode($au, $i as i64)
    };
}
macro_rules! budget_push {
    (annexb, $t:ident, $p:ident) => {
        $t.push_frame($p.width, $p.height, 8, $p.pts, $p.key, &[&$p.y, &$p.u, &$p.v])
    };
    (av1, $t:ident, $p:ident) => {
        $t.push_frame(
            $p.width,
            $p.height,
            u32::from($p.bit_depth),
            $p.pts,
            false,
            &[&le16(&$p.planes[0]), &le16(&$p.planes[1]), &le16(&$p.planes[2])],
        )
    };
    (vp9, $t:ident, $p:ident) => {
        $t.push_frame($p.width, $p.height, $p.bit_depth, $p.pts, $p.key, &[&$p.y.to_le_bytes(), &$p.u.to_le_bytes(), &$p.v.to_le_bytes()])
    };
}
macro_rules! hevc_budget_driver {
    ($m:ident) => {
        pub fn hevc_budget(bytes: &[u8]) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let Some(b) = bytes.get(..8).map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])) else { return t };
                let mut dec = $crate::sides::api::$m::hevc::Decoder::with_threads(1);
                dec.set_memory_budget(b);
                for (i, au) in split_hevc(&bytes[8..]).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(au, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(
                                    p.width,
                                    p.height,
                                    p.bit_depth,
                                    p.pts,
                                    p.key,
                                    &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()],
                                );
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                for p in &dec.flush() {
                    t.push_frame(p.width, p.height, p.bit_depth, p.pts, p.key, &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()]);
                }
                t
            })
        }
    };
}

macro_rules! h264_driver {
    ($m:ident) => {
        pub fn h264(bytes: &[u8]) -> Result<Trace, String> {
            h264_with(bytes, None)
        }
        /// One decoder thread: output cannot depend on thread timing.
        pub fn h264_st(bytes: &[u8]) -> Result<Trace, String> {
            h264_with(bytes, Some(1))
        }
        fn h264_with(bytes: &[u8], threads: Option<usize>) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let mut dec = match threads {
                    Some(n) => $crate::sides::api::$m::h264::Decoder::with_threads(n),
                    None => $crate::sides::api::$m::h264::Decoder::new(),
                };
                for (i, au) in split_h264(bytes).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(au, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(p.width, p.height, 8, p.pts, p.key, &[&p.y, &p.u, &p.v]);
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                for p in &dec.flush() {
                    t.push_frame(p.width, p.height, 8, p.pts, p.key, &[&p.y, &p.u, &p.v]);
                }
                if let Some(e) = dec.take_error() {
                    t.error(&e, e.to_string(), -1);
                }
                t
            })
        }
    };
}

macro_rules! h264_avcc_driver {
    ($m:ident) => {
        /// `[u32 BE avcC length][avcC][Annex-B]`: configure from the record (outcome and NAL length
        /// size are recorded), then decode the stream.
        pub fn h264_avcc(bytes: &[u8]) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let Some(len) = bytes.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize) else { return t };
                let Some(rec) = bytes.get(4..4 + len) else { return t };
                let rest = &bytes[4 + len..];
                let mut dec = $crate::sides::api::$m::h264::Decoder::with_threads(1);
                match dec.configure_avcc(rec) {
                    Ok(_) => t.ranges.push(1),
                    Err(e) => {
                        t.ranges.push(0);
                        t.error(&e, e.to_string(), 0);
                    }
                }
                t.ranges.push(dec.nal_length_size().map_or(0, |n| n as u32));
                for (i, au) in split_h264(rest).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(au, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(p.width, p.height, 8, p.pts, p.key, &[&p.y, &p.u, &p.v]);
                            }
                        }
                        Err(e) => {
                            if matches!(t.end, End::Complete) {
                                t.error(&e, e.to_string(), i as i64 + 1);
                            }
                            break;
                        }
                    }
                }
                t
            })
        }
    };
}

macro_rules! hevc_hvcc_driver {
    ($m:ident) => {
        /// `[u32 BE hvcC length][hvcC][Annex-B]`: configure from the record (outcome and NAL length
        /// size are recorded), then decode the stream.
        pub fn hevc_hvcc(bytes: &[u8]) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let Some(len) = bytes.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize) else { return t };
                let Some(rec) = bytes.get(4..4 + len) else { return t };
                let rest = &bytes[4 + len..];
                let mut dec = $crate::sides::api::$m::hevc::Decoder::with_threads(1);
                match dec.configure_hvcc(rec) {
                    Ok(_) => t.ranges.push(1),
                    Err(e) => {
                        t.ranges.push(0);
                        t.error(&e, e.to_string(), 0);
                    }
                }
                t.ranges.push(dec.nal_length_size().map_or(0, |n| n as u32));
                for (i, au) in split_hevc(rest).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(au, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(
                                    p.width,
                                    p.height,
                                    p.bit_depth,
                                    p.pts,
                                    p.key,
                                    &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()],
                                );
                            }
                        }
                        Err(e) => {
                            if matches!(t.end, End::Complete) {
                                t.error(&e, e.to_string(), i as i64 + 1);
                            }
                            break;
                        }
                    }
                }
                t
            })
        }
    };
}

macro_rules! hevc_driver {
    ($m:ident) => {
        pub fn hevc(bytes: &[u8]) -> Result<Trace, String> {
            hevc_with(bytes, None)
        }
        /// One decoder thread: output cannot depend on thread timing.
        pub fn hevc_st(bytes: &[u8]) -> Result<Trace, String> {
            hevc_with(bytes, Some(1))
        }
        fn hevc_with(bytes: &[u8], threads: Option<usize>) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let mut dec = match threads {
                    Some(n) => $crate::sides::api::$m::hevc::Decoder::with_threads(n),
                    None => $crate::sides::api::$m::hevc::Decoder::new(),
                };
                for (i, au) in split_hevc(bytes).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(au, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(
                                    p.width,
                                    p.height,
                                    p.bit_depth,
                                    p.pts,
                                    p.key,
                                    &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()],
                                );
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                for p in &dec.flush() {
                    t.push_frame(p.width, p.height, p.bit_depth, p.pts, p.key, &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()]);
                }
                if let Some(e) = dec.take_error() {
                    t.error(&e, e.to_string(), -1);
                }
                t
            })
        }
    };
}

macro_rules! av1_driver {
    ($m:ident) => {
        pub fn av1(bytes: &[u8]) -> Result<Trace, String> {
            av1_with(bytes, None)
        }
        /// One decoder thread: output cannot depend on thread timing.
        pub fn av1_st(bytes: &[u8]) -> Result<Trace, String> {
            av1_with(bytes, Some(1))
        }
        fn av1_with(bytes: &[u8], threads: Option<usize>) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let mut dec = match threads {
                    Some(n) => $crate::sides::api::$m::av1::Decoder::with_threads(n),
                    None => $crate::sides::api::$m::av1::Decoder::new(),
                };
                for (i, tu) in ivf_frames(bytes).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode_pts(tu, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(
                                    p.width,
                                    p.height,
                                    u32::from(p.bit_depth),
                                    p.pts,
                                    false,
                                    &[&le16(&p.planes[0]), &le16(&p.planes[1]), &le16(&p.planes[2])],
                                );
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                for p in &dec.flush() {
                    t.push_frame(
                        p.width,
                        p.height,
                        u32::from(p.bit_depth),
                        p.pts,
                        false,
                        &[&le16(&p.planes[0]), &le16(&p.planes[1]), &le16(&p.planes[2])],
                    );
                }
                t
            })
        }
    };
}

macro_rules! vp8_driver {
    ($m:ident) => {
        pub fn vp8_st(bytes: &[u8]) -> Result<Trace, String> {
            vp8(bytes)
        }
        pub fn vp8(bytes: &[u8]) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let mut dec = $crate::sides::api::$m::vp8::Decoder::new();
                for (i, f) in ivf_frames(bytes).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(f, i as i64) {
                        Ok(Some(p)) => t.push_frame(p.width, p.height, 8, p.pts, p.key, &[&p.y, &p.u, &p.v]),
                        Ok(None) => {}
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                t
            })
        }
    };
}

macro_rules! vp9_driver {
    ($m:ident) => {
        pub fn vp9(bytes: &[u8]) -> Result<Trace, String> {
            vp9_with(bytes, None)
        }
        /// One decoder thread: output cannot depend on thread timing.
        pub fn vp9_st(bytes: &[u8]) -> Result<Trace, String> {
            vp9_with(bytes, Some(1))
        }
        fn vp9_with(bytes: &[u8], threads: Option<usize>) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let mut dec = match threads {
                    Some(n) => $crate::sides::api::$m::vp9::Decoder::with_threads(n),
                    None => $crate::sides::api::$m::vp9::Decoder::new(),
                };
                for (i, f) in ivf_frames(bytes).into_iter().enumerate() {
                    t.units += 1;
                    match dec.decode(f, i as i64) {
                        Ok(pics) => {
                            for p in &pics {
                                t.push_frame(
                                    p.width,
                                    p.height,
                                    p.bit_depth,
                                    p.pts,
                                    p.key,
                                    &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()],
                                );
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                for p in &dec.flush() {
                    t.push_frame(p.width, p.height, p.bit_depth, p.pts, p.key, &[&p.y.to_le_bytes(), &p.u.to_le_bytes(), &p.v.to_le_bytes()]);
                }
                t
            })
        }
    };
}

macro_rules! opus_driver {
    ($m:ident) => {
        /// An Ogg Opus file through the multistream decoder (OpusHead from the first packet).
        pub fn opus_ogg(bytes: &[u8]) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let packets = ogg_packets(bytes);
                let Some(head) = packets.first() else {
                    t.error(&"NoHead", "no OpusHead packet".into(), 0);
                    return t;
                };
                let mut dec = match $crate::sides::api::$m::opus::Decoder::new(head) {
                    Ok(d) => d,
                    Err(e) => {
                        t.error(&e, e.to_string(), 0);
                        return t;
                    }
                };
                for (i, pkt) in packets.iter().enumerate().skip(2) {
                    t.units += 1;
                    match dec.decode(Some(pkt)) {
                        Ok(chans) => {
                            for c in &chans {
                                t.pcm.extend_from_slice(c);
                            }
                            t.ranges.push(dec.final_range());
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                t
            })
        }
        /// An RFC 6716 `.bit` vector through the stream decoder (48 kHz, `channels` = byte 0 of the input name is
        /// not available here, so stereo is used; the mono downmix scenario is `opus_bit_mono`).
        pub fn opus_bit_stereo(bytes: &[u8]) -> Result<Trace, String> {
            opus_bit(bytes, 2, false)
        }
        pub fn opus_bit_mono(bytes: &[u8]) -> Result<Trace, String> {
            // mono output is scored against the phase-inversion-free reference (`...m.dec`), as the crate's own test does
            opus_bit(bytes, 1, true)
        }
        pub fn opus_bit_stereo_noinv(bytes: &[u8]) -> Result<Trace, String> {
            opus_bit(bytes, 2, true)
        }
        fn opus_bit(bytes: &[u8], channels: usize, no_inv: bool) -> Result<Trace, String> {
            guarded(|| {
                let mut t = Trace::new();
                let mut dec = match $crate::sides::api::$m::opus::StreamDecoder::new(48000, channels) {
                    Ok(d) => d,
                    Err(e) => {
                        t.error(&e, e.to_string(), 0);
                        return t;
                    }
                };
                if no_inv {
                    dec.set_phase_inversion_disabled(true);
                }
                for (i, (pkt, _rng)) in bit_packets(bytes).iter().enumerate() {
                    t.units += 1;
                    let r = if pkt.is_empty() { dec.decode_interleaved(None, &mut t.pcm) } else { dec.decode_interleaved(Some(pkt), &mut t.pcm) };
                    match r {
                        Ok(_) => {
                            if !pkt.is_empty() {
                                t.ranges.push(dec.final_range());
                            }
                        }
                        Err(e) => {
                            t.error(&e, e.to_string(), i as i64);
                            return t;
                        }
                    }
                }
                t
            })
        }
    };
}

/// The implementations under test. `new` is the workspace crates; the others are the apps' forks at
/// their pre-swap commits (see Cargo.toml).
pub mod api {
    pub mod new {
        pub use craft_av1 as av1;
        pub use craft_h264 as h264;
        pub use craft_hevc as hevc;
        pub use craft_opus as opus;
        pub use craft_vp8 as vp8;
        pub use craft_vp9 as vp9;
    }
    pub mod pre {
        pub use craft_av1_pre as av1;
        pub use craft_h264_pre as h264;
        pub use craft_hevc_pre as hevc;
        pub use craft_opus_pre as opus;
        pub use craft_vp8_pre as vp8;
        pub use craft_vp9_pre as vp9;
    }
    pub mod fc {
        pub use fc_av1 as av1;
        pub use fc_h264 as h264;
        pub use fc_hevc as hevc;
        pub use fc_opus as opus;
        pub use fc_vp8 as vp8;
        pub use fc_vp9 as vp9;
    }
    pub mod dc {
        pub use dc_av1 as av1;
        pub use dc_h264 as h264;
        pub use dc_hevc as hevc;
        pub use dc_opus as opus;
        pub use dc_vp9 as vp9;
    }
    pub mod sc {
        pub use sc_video::h264;
    }
}

pub mod new_side {
    use super::*;
    h264_driver!(new);
    budget_driver!(new, h264, h264_budget, split_h264, annexb);
    hevc_budget_driver!(new);
    budget_driver!(new, av1, av1_budget, ivf_frames, av1);
    budget_driver!(new, vp9, vp9_budget, ivf_frames, vp9);
    h264_avcc_driver!(new);
    hevc_driver!(new);
    hevc_hvcc_driver!(new);
    av1_driver!(new);
    vp8_driver!(new);
    vp9_driver!(new);
    opus_driver!(new);
}
pub mod pre_side {
    use super::*;
    h264_driver!(pre);
    h264_avcc_driver!(pre);
    hevc_driver!(pre);
    hevc_hvcc_driver!(pre);
    av1_driver!(pre);
    vp8_driver!(pre);
    vp9_driver!(pre);
    opus_driver!(pre);
}
pub mod fc_side {
    use super::*;
    h264_driver!(fc);
    h264_avcc_driver!(fc);
    hevc_driver!(fc);
    hevc_hvcc_driver!(fc);
    av1_driver!(fc);
    vp8_driver!(fc);
    vp9_driver!(fc);
    opus_driver!(fc);
}
/// deckcraft has no vp8 fork.
pub mod dc_side {
    use super::*;
    h264_driver!(dc);
    budget_driver!(dc, h264, h264_budget, split_h264, annexb);
    hevc_budget_driver!(dc);
    budget_driver!(dc, av1, av1_budget, ivf_frames, av1);
    budget_driver!(dc, vp9, vp9_budget, ivf_frames, vp9);
    h264_avcc_driver!(dc);
    hevc_driver!(dc);
    hevc_hvcc_driver!(dc);
    av1_driver!(dc);
    vp9_driver!(dc);
    opus_driver!(dc);
}
/// soundcraft carries only h264 (a module of soundcraft-video).
pub mod sc_side {
    use super::*;
    h264_driver!(sc);
    h264_avcc_driver!(sc);
}

pub type Drv = fn(&[u8]) -> Result<Trace, String>;

macro_rules! table {
    ($side:ident, [$(($n:ident, $nst:ident)),*]) => {
        pub fn $side(name: &str, single_thread: bool) -> Option<Drv> {
            match (name, single_thread) {
                $((stringify!($n), false) => Some($side::$n as Drv), (stringify!($n), true) => Some($side::$nst as Drv),)*
                _ => None,
            }
        }
    };
}

/// Look a driver up by side (`new`, `pre`, `fc`, `dc`, `sc`), name and threading.
pub mod lookup {
    use super::*;
    use super::{dc_side as dc, fc_side as fc, new_side as new, pre_side as pre, sc_side as sc};
    table!(
        new,
        [
            (h264_budget, h264_budget),
            (hevc_budget, hevc_budget),
            (av1_budget, av1_budget),
            (vp9_budget, vp9_budget),
            (h264, h264_st),
            (h264_avcc, h264_avcc),
            (hevc, hevc_st),
            (hevc_hvcc, hevc_hvcc),
            (av1, av1_st),
            (vp8, vp8_st),
            (vp9, vp9_st),
            (opus_ogg, opus_ogg),
            (opus_bit_stereo, opus_bit_stereo),
            (opus_bit_mono, opus_bit_mono),
            (opus_bit_stereo_noinv, opus_bit_stereo_noinv)
        ]
    );
    table!(
        pre,
        [
            (h264, h264_st),
            (h264_avcc, h264_avcc),
            (hevc, hevc_st),
            (hevc_hvcc, hevc_hvcc),
            (av1, av1_st),
            (vp8, vp8_st),
            (vp9, vp9_st),
            (opus_ogg, opus_ogg),
            (opus_bit_stereo, opus_bit_stereo),
            (opus_bit_mono, opus_bit_mono),
            (opus_bit_stereo_noinv, opus_bit_stereo_noinv)
        ]
    );
    table!(
        fc,
        [
            (h264, h264_st),
            (h264_avcc, h264_avcc),
            (hevc, hevc_st),
            (hevc_hvcc, hevc_hvcc),
            (av1, av1_st),
            (vp8, vp8_st),
            (vp9, vp9_st),
            (opus_ogg, opus_ogg),
            (opus_bit_stereo, opus_bit_stereo),
            (opus_bit_mono, opus_bit_mono),
            (opus_bit_stereo_noinv, opus_bit_stereo_noinv)
        ]
    );
    table!(
        dc,
        [
            (h264_budget, h264_budget),
            (hevc_budget, hevc_budget),
            (av1_budget, av1_budget),
            (vp9_budget, vp9_budget),
            (h264, h264_st),
            (h264_avcc, h264_avcc),
            (hevc, hevc_st),
            (hevc_hvcc, hevc_hvcc),
            (av1, av1_st),
            (vp9, vp9_st),
            (opus_ogg, opus_ogg),
            (opus_bit_stereo, opus_bit_stereo),
            (opus_bit_mono, opus_bit_mono),
            (opus_bit_stereo_noinv, opus_bit_stereo_noinv)
        ]
    );
    table!(sc, [(h264, h264_st), (h264_avcc, h264_avcc)]);
}
