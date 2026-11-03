// Provenance: texturecraft/xtask/src/ico.rs (checked ICO/ICNS packing); craft-launcher/xtask/src/icns.rs (same slot map).
use crate::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct IcoConfig {
    pub root: PathBuf,
    pub source_pngs: Vec<PathBuf>,
    pub output_path: Option<PathBuf>,
    /// Empty accepts every valid format size; otherwise inputs must match this ordered list.
    pub sizes: Vec<u32>,
    pub icns: bool,
}

impl Default for IcoConfig {
    fn default() -> Self {
        Self { root: ".".into(), source_pngs: Vec::new(), output_path: None, sizes: Vec::new(), icns: false }
    }
}

pub type Config = IcoConfig;

pub fn cli_help() -> &'static str {
    "ico/icns <out.ico|out.icns> <in.png>… (or configured paths)"
}

pub fn run(cfg: &Config, args: &[String]) -> Result {
    let (output, inputs) = if args.is_empty() {
        (cfg.output_path.clone().ok_or_else(|| cli_help().to_string())?, cfg.source_pngs.clone())
    } else {
        (PathBuf::from(&args[0]), args[1..].iter().map(PathBuf::from).collect())
    };
    let pngs = inputs
        .iter()
        .map(|path| std::fs::read(cfg.root.join(path)).map_err(|error| format!("read {}: {error}", path.display())))
        .collect::<Result<Vec<_>>>()?;
    if !cfg.sizes.is_empty() {
        let sizes = pngs.iter().map(|png| square(png)).collect::<Result<Vec<_>>>()?;
        if sizes != cfg.sizes {
            return Err(format!("expected icon sizes {:?}, got {sizes:?}", cfg.sizes));
        }
    }
    let bytes = if cfg.icns { pack_icns(&pngs)? } else { pack_ico(&pngs)? };
    crate::write(&cfg.root.join(&output), &bytes)?;
    println!("{}: {} images, {} bytes", output.display(), pngs.len(), bytes.len());
    Ok(())
}

pub fn run_icns(cfg: &Config, args: &[String]) -> Result {
    run(&Config { icns: true, ..cfg.clone() }, args)
}

pub use pack_ico as pack;
/// Width and height from a PNG's IHDR chunk.
pub fn png_size(png: &[u8]) -> Result<(u32, u32), String> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    let (Some(sig), Some(ihdr), Some(w), Some(h)) = (png.get(..8), png.get(12..16), png.get(16..20), png.get(20..24)) else {
        return Err("not a PNG file".into());
    };
    if sig != SIG || ihdr != b"IHDR" {
        return Err("not a PNG file".into());
    }
    let be = |b: &[u8]| b.iter().fold(0u32, |acc, &x| (acc << 8) | u32::from(x));
    Ok((be(w), be(h)))
}

fn square(png: &[u8]) -> Result<u32, String> {
    match png_size(png)? {
        (w, h) if w == h && w > 0 => Ok(w),
        (w, h) => Err(format!("icon images must be square, got {w}x{h}")),
    }
}

/// An ICO file holding `pngs`, which must be square and at most 256 px.
pub fn pack_ico(pngs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(pngs.len()).ok().filter(|n| *n > 0).ok_or("need 1 to 65535 PNGs")?;
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * pngs.len();
    for png in pngs {
        let size = square(png)?;
        // One byte per dimension; 256 is written as 0.
        let dim = match size {
            256 => 0,
            s => u8::try_from(s).map_err(|_| format!("ICO images must be at most 256 px, got {s}"))?,
        };
        out.extend_from_slice(&[dim, dim, 0, 0]); // width, height, palette size, reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&u32::try_from(png.len()).map_err(|_| "PNG too large")?.to_le_bytes());
        out.extend_from_slice(&u32::try_from(offset).map_err(|_| "ICO too large")?.to_le_bytes());
        offset = offset.checked_add(png.len()).ok_or("ICO too large")?;
    }
    for png in pngs {
        out.extend_from_slice(png);
    }
    Ok(out)
}

/// ICNS element types holding PNG data, by pixel size (the 1x and 2x slots iconutil writes).
fn icns_types(size: u32) -> Result<&'static [&'static [u8; 4]], String> {
    Ok(match size {
        16 => &[b"icp4"],
        32 => &[b"icp5", b"ic11"],
        64 => &[b"icp6", b"ic12"],
        128 => &[b"ic07"],
        256 => &[b"ic08", b"ic13"],
        512 => &[b"ic09", b"ic14"],
        1024 => &[b"ic10"],
        s => return Err(format!("ICNS images must be 16, 32, 64, 128, 256, 512 or 1024 px, got {s}")),
    })
}

/// An ICNS file holding `pngs`, each stored in every slot of its size.
pub fn pack_icns(pngs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if pngs.is_empty() {
        return Err("need at least one PNG".into());
    }
    let mut body = Vec::new();
    for png in pngs {
        for ty in icns_types(square(png)?)? {
            body.extend_from_slice(*ty);
            let length = png.len().checked_add(8).ok_or("PNG too large")?;
            body.extend_from_slice(&u32::try_from(length).map_err(|_| "PNG too large")?.to_be_bytes());
            body.extend_from_slice(png);
        }
    }
    let mut out = b"icns".to_vec();
    let length = body.len().checked_add(8).ok_or("ICNS too large")?;
    out.extend_from_slice(&u32::try_from(length).map_err(|_| "ICNS too large")?.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}
