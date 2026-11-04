//! Exact comparison of two traces. Every difference becomes `Verdict::Exceeds` with a metric that
//! names what differed (`+`-joined, fixed order), so the registry can map it to a fix; nothing is
//! "within tolerance" silently.
//!
//! Metrics, in order: `end_<old>_<new>` (how decoding ended: `ok`, `err-<Class>`, `panic`),
//! `err_at` (same error class at a different unit), `err_msg` (same class and unit, other
//! message), `frames` (picture count), `frame_meta` (size, bit depth, pts or key flag),
//! `plane_bytes` (a plane's bytes; value = pictures that differ), `pcm_len`, `pcm_bytes` (value =
//! max absolute sample difference), `range_state` (Opus final range per packet).

use crate::trace::{End, Trace};
use craft_compare::Verdict;

#[derive(Clone, Debug, Default)]
pub struct Diff {
    pub aspects: Vec<String>,
    /// Where the difference comes from, using the shared crate before its hardening commit (`pre`):
    /// `fix` (old == pre != new: a hardening commit changed it), `drift` (old != pre == new: the fork
    /// differs from the verbatim shared crate), `both`, or `nopre`.
    pub attr: &'static str,
    pub value: f64,
    pub detail: String,
}

fn bytes_to_u16(b: &[u8]) -> impl Iterator<Item = i64> + '_ {
    b.chunks_exact(2).map(|c| i64::from(u16::from_le_bytes([c[0], c[1]])))
}

impl Trace {
    /// Same ending class, pictures and samples (ignoring the attribution trace).
    pub fn same_as(&self, other: &Trace) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.pre = None;
        b.pre = None;
        diff_core(&a, &b).aspects.is_empty()
    }
}

pub fn diff(old: &Trace, new: &Trace) -> Diff {
    let mut d = diff_core(old, new);
    d.attr = attribute(old, new);
    d
}

fn attribute(old: &Trace, new: &Trace) -> &'static str {
    match &new.pre {
        None => "nopre",
        Some(pre) => match (old.same_as(pre), pre.same_as(new)) {
            (true, false) => "fix",
            (false, true) => "drift",
            (false, false) => "both",
            (true, true) => "same",
        },
    }
}

fn diff_core(old: &Trace, new: &Trace) -> Diff {
    let mut d = Diff::default();
    // 1. how it ended
    if old.end.label() != new.end.label() {
        d.aspects.push(format!("end_{}_{}", old.end.label(), new.end.label()));
        d.detail += &format!("old {:?}; new {:?}; ", old.end, new.end);
    } else if let (End::Error { at: a, msg: ma, .. }, End::Error { at: b, msg: mb, .. }) = (&old.end, &new.end) {
        if a != b {
            d.aspects.push("err_at".into());
            d.detail += &format!("error at unit {a} vs {b}; ");
        } else if ma != mb {
            d.aspects.push("err_msg".into());
            d.detail += &format!("message {ma:?} vs {mb:?}; ");
        }
    }
    // 2. pictures
    if old.frames.len() != new.frames.len() {
        d.aspects.push("frames".into());
        d.detail += &format!("{} vs {} pictures; ", old.frames.len(), new.frames.len());
    }
    let mut meta = 0usize;
    let mut bad_frames = 0usize;
    let mut worst = 0i64;
    let mut first: Option<String> = None;
    for (i, (a, b)) in old.frames.iter().zip(&new.frames).enumerate() {
        if (a.w, a.h, a.depth, a.pts, a.key) != (b.w, b.h, b.depth, b.pts, b.key) {
            meta += 1;
            first.get_or_insert_with(|| {
                format!(
                    "picture {i} meta {}x{} d{} pts{} key{} vs {}x{} d{} pts{} key{}",
                    a.w, a.h, a.depth, a.pts, a.key, b.w, b.h, b.depth, b.pts, b.key
                )
            });
            continue;
        }
        let mut differs = false;
        for (pi, (pa, pb)) in a.planes.iter().zip(&b.planes).enumerate() {
            if pa.len != pb.len || pa.sha != pb.sha {
                differs = true;
                let msg = match (&pa.data, &pb.data) {
                    (Some(x), Some(y)) if x.len() == y.len() => {
                        let wide = a.depth > 8;
                        let (n, m) = if wide {
                            bytes_to_u16(x)
                                .zip(bytes_to_u16(y))
                                .fold((0usize, 0i64), |(n, m), (p, q)| (n + usize::from(p != q), m.max((p - q).abs())))
                        } else {
                            x.iter()
                                .zip(y)
                                .fold((0usize, 0i64), |(n, m), (&p, &q)| (n + usize::from(p != q), m.max((i64::from(p) - i64::from(q)).abs())))
                        };
                        worst = worst.max(m);
                        format!("picture {i} plane {pi}: {n} samples differ, max abs {m}")
                    }
                    _ => format!("picture {i} plane {pi}: hash differs (len {} vs {})", pa.len, pb.len),
                };
                first.get_or_insert(msg);
                break;
            }
        }
        bad_frames += usize::from(differs);
    }
    if meta > 0 {
        d.aspects.push("frame_meta".into());
        d.value = meta as f64;
    }
    if bad_frames > 0 {
        d.aspects.push("plane_bytes".into());
        d.value = bad_frames as f64;
        d.detail += &format!("{bad_frames} pictures differ (max abs sample diff {worst}); ");
    }
    if let Some(f) = first {
        d.detail += &format!("first: {f}; ");
    }
    // 3. opus PCM and range state
    if old.pcm.len() != new.pcm.len() {
        d.aspects.push("pcm_len".into());
        d.detail += &format!("{} vs {} samples; ", old.pcm.len(), new.pcm.len());
    }
    let mut max_abs = 0f64;
    let mut count = 0usize;
    for (a, b) in old.pcm.iter().zip(&new.pcm) {
        if a.to_bits() != b.to_bits() {
            count += 1;
            let e = f64::from((a - b).abs());
            max_abs = if e.is_nan() { f64::INFINITY } else { max_abs.max(e) };
        }
    }
    if count > 0 {
        d.aspects.push("pcm_bytes".into());
        d.value = max_abs;
        d.detail += &format!("{count} PCM samples differ, max abs {max_abs:.3e}; ");
    }
    if old.ranges != new.ranges {
        d.aspects.push("range_state".into());
        let at = old.ranges.iter().zip(&new.ranges).position(|(a, b)| a != b);
        d.detail += &format!("range state differs (first at packet index {at:?}, {} vs {} states); ", old.ranges.len(), new.ranges.len());
    }
    if !d.aspects.is_empty() && d.value == 0.0 {
        d.value = 1.0;
    }
    d
}

fn norm(s: &str) -> String {
    let mut out = String::new();
    let mut digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !digit {
                out.push('N');
            }
            digit = true;
        } else {
            digit = false;
            out.push(c);
        }
    }
    out.chars().take(70).collect()
}

fn site(e: &End) -> Option<String> {
    match e {
        End::Panic { loc, .. } => Some(format!("panic@{}", loc.rsplit('/').next().unwrap_or("?").split(':').next().unwrap_or("?"))),
        End::Error { msg, .. } => Some(norm(msg)),
        End::Complete => None,
    }
}

/// What the registry keys on beyond the aspect list: the panic site or error message each side ended with
/// (digits collapsed to N), when the two ended differently.
pub fn signature(old: &Trace, new: &Trace, aspects: &[String]) -> String {
    if !aspects.iter().any(|a| a.starts_with("end_") || a == "err_msg") {
        return String::new();
    }
    let (o, n) = (site(&old.end), site(&new.end));
    format!("|old={}|new={}", o.unwrap_or_else(|| "ok".into()), n.unwrap_or_else(|| "ok".into()))
}

pub fn judge(old: &Trace, new: &Trace) -> Verdict {
    let d = diff(old, new);
    if d.aspects.is_empty() {
        Verdict::Identical
    } else {
        Verdict::Exceeds { metric: format!("{}|{}{}", d.aspects.join("+"), d.attr, signature(old, new, &d.aspects)), value: d.value, limit: 0.0 }
    }
}
