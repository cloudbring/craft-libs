//! The ffmpeg fixture matrix. h264, hevc, vp8 and vp9 come from the crates' own conformance
//! tables (`matrix_data.rs`, extracted by `scripts/extract_matrix.py`), so the oracle decodes the
//! same encoder configurations the crates test against; av1 and opus use the encoder settings of
//! the crates' oracle tests. ffmpeg discovery, the skip-or-fail rule and the fixture directory
//! come from craft-media-testkit.

use craft_media_testkit as kit;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One entry of the table-driven matrix (h264, hevc, vp8, vp9).
pub struct Fx {
    pub codec: &'static str,
    pub name: &'static str,
    pub source: &'static str,
    pub w: u32,
    pub h: u32,
    pub frames: u32,
    pub filter: &'static str,
    pub pix_fmt: &'static str,
    pub args: &'static [&'static str],
}

pub use crate::matrix_data::FIXTURES;

/// Streams skipped: the performance reference streams (long, 1080p/4K) add decode time, not coverage.
fn skipped(f: &Fx) -> bool {
    f.name.starts_with("bench_")
}

pub fn ext(codec: &str) -> &'static str {
    match codec {
        "h264" => "h264",
        "hevc" => "hevc",
        "opus" => "opus",
        _ => "ivf",
    }
}

fn s(x: &str) -> String {
    x.to_string()
}

struct Job {
    codec: &'static str,
    name: String,
    /// Full command-line after `-hide_banner -loglevel error -y`, with `OUT` replaced by the temp path.
    pass1: Option<Vec<String>>,
    args: Vec<String>,
}

fn base_input(f: &Fx, even: bool) -> String {
    let (w, h) = if even { (f.w.next_multiple_of(2), f.h.next_multiple_of(2)) } else { (f.w, f.h) };
    let mut v = format!("{}=size={}x{}:rate=25", f.source, w, h);
    if even && (w != f.w || h != f.h) {
        v += &format!(",scale={}:{}", f.w, f.h);
    }
    v += &format!(",format={}", f.pix_fmt);
    if !f.filter.is_empty() {
        v.push(',');
        v += f.filter;
    }
    v
}

fn table_jobs() -> Vec<Job> {
    let mut jobs = Vec::new();
    for f in FIXTURES.iter().filter(|f| !skipped(f)) {
        let two_pass = f.args.first() == Some(&"2pass");
        let user: Vec<String> = f.args.iter().filter(|a| **a != "2pass").map(|a| s(a)).collect();
        let vf = base_input(f, f.codec == "vp8");
        let mut head = vec![s("-f"), s("lavfi"), s("-i"), vf, s("-frames:v"), f.frames.to_string()];
        let has_enc = f.args.contains(&"-c:v");
        let mut enc = Vec::new();
        match f.codec {
            "h264" => {
                if !has_enc {
                    enc.extend([s("-c:v"), s("libx264")]);
                }
                enc.extend(user.clone());
                enc.extend([s("-f"), s("h264")]);
            }
            "hevc" => {
                if !has_enc {
                    enc.extend([s("-c:v"), s("libx265")]);
                }
                enc.extend([s("-pix_fmt"), s(f.pix_fmt)]);
                enc.extend(user.clone());
                enc.extend([s("-f"), s("hevc")]);
            }
            "vp8" => {
                enc.extend([s("-c:v"), s("libvpx")]);
                enc.extend(user.clone());
                enc.extend([s("-f"), s("ivf")]);
            }
            _ => {
                enc.extend([s("-c:v"), s("libvpx-vp9"), s("-pix_fmt"), s(f.pix_fmt)]);
                enc.extend(user.clone());
                enc.extend([s("-f"), s("ivf")]);
            }
        }
        let mut pass1 = None;
        if two_pass {
            let mut p1 = head.clone();
            p1.extend(enc.iter().take(enc.len() - 2).cloned());
            p1.extend([s("-pass"), s("1"), s("-passlogfile"), s("PASSLOG"), s("-f"), s("null"), s("-")]);
            pass1 = Some(p1);
            head.extend(enc.iter().take(enc.len() - 2).cloned());
            head.extend([s("-pass"), s("2"), s("-passlogfile"), s("PASSLOG")]);
            head.extend(enc.iter().skip(enc.len() - 2).cloned());
        } else {
            head.extend(enc);
        }
        jobs.push(Job { codec: f.codec, name: f.name.to_string(), pass1, args: head });
    }
    jobs
}

fn av1_jobs() -> Vec<Job> {
    // (name, lavfi, frames, pix_fmt, encoder, extra args): the Spec lists of oracle_intra/oracle_inter.
    let nf = "enable-cdef=0:enable-restoration=0:enable-dlf=0:keyint=1";
    let p = |preset: &str, crf: &str, params: Option<&str>| -> Vec<String> {
        let mut v = vec![s("-preset"), s(preset), s("-crf"), s(crf)];
        if let Some(x) = params {
            v.extend([s("-svtav1-params"), s(x)]);
        }
        v
    };
    let mut specs: Vec<(&str, &str, u32, &str, &str, Vec<String>)> = vec![
        ("intra_nofilt_testsrc_128", "testsrc2=s=128x128:r=25", 1, "yuv420p", "libsvtav1", p("8", "30", Some(nf))),
        ("intra_nofilt_mandel_352x288_p4", "mandelbrot=s=352x288:r=25", 2, "yuv420p", "libsvtav1", p("4", "20", Some(nf))),
        ("intra_nofilt_testsrc_odd_p2", "testsrc2=s=203x117:r=25", 2, "yuv420p", "libsvtav1", p("2", "35", Some(nf))),
        ("intra_nofilt_noise_p6", "testsrc2=s=320x240:r=25,noise=alls=30:allf=t", 2, "yuv420p", "libsvtav1", p("6", "10", Some(nf))),
        ("intra_nofilt_10bit_p4", "mandelbrot=s=256x192:r=25", 2, "yuv420p10le", "libsvtav1", p("4", "25", Some(nf))),
        ("intra_nofilt_720p_p8", "testsrc2=s=1280x720:r=25", 1, "yuv420p", "libsvtav1", p("8", "40", Some(nf))),
        (
            "intra_dlf_mandel",
            "mandelbrot=s=352x288:r=25",
            2,
            "yuv420p",
            "libsvtav1",
            p("6", "40", Some("enable-cdef=0:enable-restoration=0:keyint=1")),
        ),
        ("intra_cdef_testsrc", "testsrc2=s=352x288:r=25", 2, "yuv420p", "libsvtav1", p("6", "40", Some("enable-restoration=0:keyint=1"))),
        ("intra_lr_mandel", "mandelbrot=s=352x288:r=25", 2, "yuv420p", "libsvtav1", p("4", "40", Some("enable-cdef=0:keyint=1"))),
        ("intra_all_testsrc", "testsrc2=s=640x360:r=25", 2, "yuv420p", "libsvtav1", p("4", "45", Some("keyint=1"))),
        ("intra_all_10bit", "mandelbrot=s=320x240:r=25", 2, "yuv420p10le", "libsvtav1", p("4", "40", Some("keyint=1"))),
        (
            "intra_qmatrix_noise",
            "testsrc2=s=352x288:r=25,noise=alls=10:allf=t",
            2,
            "yuv420p",
            "libsvtav1",
            p("6", "30", Some("enable-qm=1:qm-min=0:qm-max=8:keyint=1")),
        ),
        ("intra_all_noise_odd", "testsrc2=s=250x150:r=25,noise=alls=20:allf=t", 2, "yuv420p", "libsvtav1", p("3", "35", Some("keyint=1"))),
        (
            "intra_all_grad_noise_10bit",
            "gradients=s=640x360:r=25,noise=alls=25:allf=t",
            1,
            "yuv420p10le",
            "libsvtav1",
            p("1", "50", Some("keyint=1")),
        ),
        ("inter_testsrc_p8", "testsrc2=s=320x240:r=25", 10, "yuv420p", "libsvtav1", p("8", "35", None)),
        ("inter_testsrc_p3", "testsrc2=s=352x288:r=25", 24, "yuv420p", "libsvtav1", p("3", "30", None)),
        ("inter_mandel_10bit_p5", "mandelbrot=s=416x240:r=25", 16, "yuv420p10le", "libsvtav1", p("5", "40", None)),
        ("inter_noise_odd_p6", "testsrc2=s=203x117:r=25,noise=alls=15:allf=t", 12, "yuv420p", "libsvtav1", p("6", "45", None)),
        (
            "inter_superres",
            "testsrc2=s=416x240:r=25",
            8,
            "yuv420p",
            "libsvtav1",
            p("6", "35", Some("superres-mode=1:superres-denom=12:superres-kf-denom=13")),
        ),
        ("inter_film_grain", "testsrc2=s=352x288:r=25,noise=alls=20:allf=t", 8, "yuv420p", "libsvtav1", p("6", "35", Some("film-grain=10"))),
        ("inter_qmatrix_10bit", "mandelbrot=s=352x288:r=25", 8, "yuv420p10le", "libsvtav1", p("6", "35", Some("enable-qm=1:qm-min=0:qm-max=8"))),
        (
            "inter_tiles_4x2",
            "testsrc2=s=1280x720:r=25,noise=alls=4:allf=t",
            8,
            "yuv420p",
            "libsvtav1",
            p("8", "32", Some("tile-columns=2:tile-rows=1")),
        ),
        ("inter_tiles_odd_10bit", "mandelbrot=s=650x370:r=25", 8, "yuv420p10le", "libsvtav1", p("6", "32", Some("tile-columns=1:tile-rows=1"))),
    ];
    let aom = |pf| vec![s("-cpu-used"), s("8"), s("-crf"), s("40"), s("-enable-restoration"), s("1"), s("-pix_fmt_unused"), s(pf)];
    for (n, pf) in [("inter_aom_444", "yuv444p"), ("inter_aom_422", "yuv422p"), ("inter_aom_444_10bit", "yuv444p10le")] {
        let mut a = aom(pf);
        a.truncate(6);
        specs.push((n, "testsrc2=s=416x400:r=25,noise=alls=8:allf=t", 6, pf, "libaom-av1", a));
    }
    specs
        .into_iter()
        .map(|(name, lavfi, frames, pix, enc, extra)| {
            let mut args = vec![s("-f"), s("lavfi"), s("-i"), s(lavfi), s("-frames:v"), frames.to_string(), s("-pix_fmt"), s(pix), s("-c:v"), s(enc)];
            args.extend(extra);
            args.extend([s("-f"), s("ivf")]);
            Job { codec: "av1", name: name.to_string(), pass1: None, args }
        })
        .collect()
}

fn opus_jobs() -> Vec<Job> {
    // (name, lavfi source, channels, encoder args): bitrates, frame sizes, applications, VBR/CBR and
    // channel layouts of the crates' ffmpeg/libopus oracle cases, on lavfi signals (no raw files).
    let music = "aevalsrc=0.3*sin(2*PI*220*t)+0.2*sin(2*PI*330*t+sin(2*PI*5*t))+0.1*sin(2*PI*1760*t)*sin(2*PI*0.5*t):s=48000:d=6";
    let speech = "aevalsrc=0.5*sin(2*PI*140*t+3*sin(2*PI*4*t))*(0.6+0.4*sin(2*PI*3*t))+0.05*sin(2*PI*2900*t)*sin(2*PI*7*t):s=48000:d=6";
    let noise = "anoisesrc=d=6:c=pink:r=48000:a=0.3";
    let sweep = "sine=f=100:d=6:r=48000,aeval=val(0)*sin(2*PI*t*t*600)";
    type C = (&'static str, &'static str, u32, Vec<&'static str>);
    let cases: Vec<C> = vec![
        ("speech_mono_6k_voip", speech, 1, vec!["-b:a", "6k", "-application", "voip"]),
        ("speech_mono_12k_voip_60ms", speech, 1, vec!["-b:a", "12k", "-application", "voip", "-frame_duration", "60"]),
        ("speech_mono_16k_voip_40ms", speech, 1, vec!["-b:a", "16k", "-application", "voip", "-frame_duration", "40"]),
        ("speech_stereo_24k_voip", speech, 2, vec!["-b:a", "24k", "-application", "voip"]),
        ("music_stereo_32k_audio", music, 2, vec!["-b:a", "32k", "-application", "audio"]),
        ("music_stereo_64k_audio_10ms", music, 2, vec!["-b:a", "64k", "-frame_duration", "10"]),
        ("music_stereo_96k_lowdelay_2_5ms", music, 2, vec!["-b:a", "96k", "-application", "lowdelay", "-frame_duration", "2.5"]),
        ("music_stereo_128k_lowdelay_5ms", music, 2, vec!["-b:a", "128k", "-application", "lowdelay", "-frame_duration", "5"]),
        ("music_stereo_160k_cbr", music, 2, vec!["-b:a", "160k", "-vbr", "off"]),
        ("music_mono_256k_20ms", music, 1, vec!["-b:a", "256k"]),
        ("sweep_stereo_510k", sweep, 2, vec!["-b:a", "510k"]),
        ("music_stereo_48k_cvbr_40ms", music, 2, vec!["-b:a", "48k", "-vbr", "constrained", "-frame_duration", "40"]),
        ("music_stereo_64k_60ms", music, 2, vec!["-b:a", "64k", "-frame_duration", "60"]),
        ("music_51_256k", music, 6, vec!["-b:a", "256k", "-mapping_family", "1"]),
        ("music_51_96k_lowdelay", music, 6, vec!["-b:a", "96k", "-mapping_family", "1", "-application", "lowdelay"]),
        ("noise_stereo_96k", noise, 2, vec!["-b:a", "96k"]),
        ("noise_mono_8k_voip", noise, 1, vec!["-b:a", "8k", "-application", "voip"]),
        ("speech_stereo_16k_voip_20ms", speech, 2, vec!["-b:a", "16k", "-application", "voip", "-frame_duration", "20"]),
        ("music_stereo_24k_audio_60ms", music, 2, vec!["-b:a", "24k", "-application", "audio", "-frame_duration", "60"]),
        ("music_stereo_300k_vbr", music, 2, vec!["-b:a", "300k"]),
        ("music_quad_128k", music, 4, vec!["-b:a", "128k", "-mapping_family", "1"]),
    ];
    cases
        .into_iter()
        .map(|(name, src, ch, a)| {
            let mut args = vec![s("-f"), s("lavfi"), s("-i"), s(src), s("-ac"), ch.to_string(), s("-c:a"), s("libopus")];
            args.extend(a.iter().map(|x| s(x)));
            args.extend([s("-f"), s("ogg")]);
            Job { codec: "opus", name: name.to_string(), pass1: None, args }
        })
        .collect()
}

/// Generate every fixture missing under `root/<codec>/`. Returns (made_or_cached, skipped) names.
pub fn generate(root: &Path, only: Option<&str>) -> (Vec<String>, Vec<String>) {
    let mut ok = Vec::new();
    let mut skipped = Vec::new();
    let Some(ff) = kit::ffmpeg() else { return (ok, vec![s("ffmpeg missing")]) };
    let mut jobs = table_jobs();
    jobs.extend(av1_jobs());
    jobs.extend(opus_jobs());
    for job in jobs {
        if only.is_some_and(|o| o != job.codec) {
            continue;
        }
        let dir = root.join(job.codec);
        let _ = std::fs::create_dir_all(&dir);
        let out = dir.join(format!("{}.{}", job.name, ext(job.codec)));
        let label = format!("{}/{}", job.codec, job.name);
        let made = kit::generate_and_report(&label, std::slice::from_ref(&out), || {
            if out.is_file() {
                return Some(out.clone());
            }
            let tmp = kit::temp_path(&out);
            let log = dir.join(format!("{}.passlog", job.name));
            let log_s = log.to_string_lossy().to_string();
            if let Some(p1) = &job.pass1 {
                let args: Vec<String> = p1.iter().map(|a| if a == "PASSLOG" { log_s.clone() } else { a.clone() }).collect();
                let mut c = Command::new(&ff);
                c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(&args);
                if !c.output().is_ok_and(|o| o.status.success()) {
                    return None;
                }
            }
            let args: Vec<String> = job.args.iter().map(|a| if a == "PASSLOG" { log_s.clone() } else { a.clone() }).collect();
            let mut c = Command::new(&ff);
            c.args(["-hide_banner", "-loglevel", "error", "-y"]).args(&args).arg(&tmp);
            let r = kit::run_generator(&mut c, &tmp, &out, false);
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                if e.file_name().to_string_lossy().starts_with(&format!("{}.passlog", job.name)) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
            r
        });
        if made.is_some() {
            ok.push(label);
        } else {
            skipped.push(label);
        }
    }
    (ok, skipped)
}

pub fn fixture_root() -> PathBuf {
    std::env::var_os("CODEC_ORACLE_FIXTURES").map(PathBuf::from).unwrap_or_else(|| kit::fixture_subdir("codec-oracle"))
}
