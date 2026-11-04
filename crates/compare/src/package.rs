//! Per-part comparison of two packages: ZIP (OPC: docx/xlsx/pptx, ODF, EPUB...) or CFB (OLE2:
//! doc/xls/ppt).
//!
//! A [`Package`] is an ordered map from part name to bytes. [`compare_packages`] reports the part
//! names present on one side only, then compares every common part: XML parts (by extension
//! `.xml`/`.rels` or an `<?xml` prologue) through [`crate::xml`] so attribute order, namespace
//! prefixes and inter-element whitespace do not count, everything else byte for byte.
//!
//! Reading real containers is opt-in so the default build has no dependencies: feature `zip`
//! enables [`Package::from_zip`] through `craft-archive`, feature `cfb` enables
//! [`Package::from_cfb`] through `craft-cfb`, and [`Package::open`] picks one by magic number.

use crate::{CompareError, Verdict, bytes::compare_bytes, xml::compare_xml};
use std::collections::BTreeMap;

/// Part name to part bytes, ordered by name. ZIP directory entries are not parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Package {
    parts: BTreeMap<String, Vec<u8>>,
}

/// Container formats [`Package::open`] recognises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Zip,
    Cfb,
}

const CFB_MAGIC: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

impl Container {
    /// Sniff the container from its leading bytes. A ZIP may start with `PK\x03\x04`, or with
    /// `PK\x05\x06` when empty; self-extracting prefixes are not recognised.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&CFB_MAGIC) {
            Some(Self::Cfb)
        } else if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
            Some(Self::Zip)
        } else {
            None
        }
    }
}

impl Package {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from `(name, bytes)` pairs; a repeated name keeps the last.
    pub fn from_parts<N: Into<String>, D: Into<Vec<u8>>>(parts: impl IntoIterator<Item = (N, D)>) -> Self {
        Self { parts: parts.into_iter().map(|(n, d)| (n.into(), d.into())).collect() }
    }

    pub fn insert(&mut self, name: impl Into<String>, data: impl Into<Vec<u8>>) {
        self.parts.insert(name.into(), data.into());
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.parts.keys().map(String::as_str)
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.parts.get(name).map(Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Read a ZIP/OPC package with [`craft_archive::ZipLimits`] of 100 000 entries, 256 MiB per
    /// entry and 2 GiB in total.
    #[cfg(feature = "zip")]
    pub fn from_zip(bytes: &[u8]) -> Result<Self, CompareError> {
        let limits = craft_archive::ZipLimits { max_entries: 100_000, max_entry_bytes: 256 << 20, max_total_bytes: 2 << 30 };
        Self::from_zip_with(bytes, limits)
    }

    /// [`from_zip`](Self::from_zip) with explicit limits.
    #[cfg(feature = "zip")]
    pub fn from_zip_with(bytes: &[u8], limits: craft_archive::ZipLimits) -> Result<Self, CompareError> {
        let err = |e: craft_archive::ZipError| CompareError::Package(e.to_string());
        let mut zip = craft_archive::ZipReader::open(bytes, limits).map_err(err)?;
        let names: Vec<String> = zip.names().iter().filter(|n| !n.ends_with('/')).cloned().collect();
        let mut parts = BTreeMap::new();
        for name in names {
            let data = zip.read(&name).map_err(err)?.unwrap_or_default();
            parts.insert(name, data);
        }
        Ok(Self { parts })
    }

    /// Read every stream of a CFB (OLE2) container; the part name is the slash-delimited path.
    #[cfg(feature = "cfb")]
    pub fn from_cfb(bytes: &[u8]) -> Result<Self, CompareError> {
        let err = |e: craft_cfb::Error| CompareError::Package(e.to_string());
        let cfb = craft_cfb::Cfb::open(bytes).map_err(err)?;
        let mut parts = BTreeMap::new();
        for entry in cfb.entries().filter(|e| e.kind() == craft_cfb::EntryKind::Stream) {
            parts.insert(entry.path().to_string(), cfb.read_stream(entry.path()).map_err(err)?);
        }
        Ok(Self { parts })
    }

    /// Detect the container and read it. Errors when the format is unknown or its reader feature
    /// (`zip`, `cfb`) is not enabled.
    pub fn open(bytes: &[u8]) -> Result<Self, CompareError> {
        match Container::detect(bytes) {
            #[cfg(feature = "zip")]
            Some(Container::Zip) => Self::from_zip(bytes),
            #[cfg(feature = "cfb")]
            Some(Container::Cfb) => Self::from_cfb(bytes),
            #[cfg(not(feature = "zip"))]
            Some(Container::Zip) => Err(CompareError::Package("ZIP package: enable the `zip` feature".into())),
            #[cfg(not(feature = "cfb"))]
            Some(Container::Cfb) => Err(CompareError::Package("CFB package: enable the `cfb` feature".into())),
            None => Err(CompareError::Package("not a ZIP or CFB package".into())),
        }
    }
}

/// How one common part compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartStatus {
    /// Same bytes.
    Identical,
    /// XML parts whose bytes differ but whose canonical forms are equal.
    Equivalent,
    /// XML parts that differ; `path` is the path of the first difference (see [`compare_xml`]).
    XmlDiffers { path: String },
    /// Non-XML parts that differ; `first_diff` is the first differing offset.
    BytesDiffer { first_diff: usize, len_a: usize, len_b: usize },
}

impl PartStatus {
    /// True for `Identical` and `Equivalent`.
    pub fn matches(&self) -> bool {
        matches!(self, Self::Identical | Self::Equivalent)
    }
}

/// Comparison of one part present on both sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartDiff {
    pub name: String,
    pub status: PartStatus,
}

/// Result of [`compare_packages`]; every list is sorted by part name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageDiff {
    pub only_in_a: Vec<String>,
    pub only_in_b: Vec<String>,
    /// Every part present on both sides, matching or not.
    pub common: Vec<PartDiff>,
}

impl PackageDiff {
    /// Common parts that do not match.
    pub fn differing(&self) -> impl Iterator<Item = &PartDiff> {
        self.common.iter().filter(|p| !p.status.matches())
    }

    /// Same part set and every part matches (canonically for XML).
    pub fn is_equal(&self) -> bool {
        self.only_in_a.is_empty() && self.only_in_b.is_empty() && self.differing().next().is_none()
    }

    /// Strictly identical: equal and no XML part needed canonicalisation.
    pub fn is_identical(&self) -> bool {
        self.is_equal() && self.common.iter().all(|p| p.status == PartStatus::Identical)
    }

    /// Number of differences: parts on one side only plus common parts that differ.
    pub fn difference_count(&self) -> usize {
        self.only_in_a.len() + self.only_in_b.len() + self.differing().count()
    }

    /// `Identical` when [`is_identical`](Self::is_identical), `Within` (metric `package parts`,
    /// value 0) when only canonical-XML equivalences remain, otherwise `Exceeds` with the number of
    /// differences against a limit of zero.
    pub fn verdict(&self) -> Verdict {
        let n = self.difference_count();
        if n > 0 {
            Verdict::Exceeds { metric: "package parts".into(), value: n as f64, limit: 0.0 }
        } else if self.is_identical() {
            Verdict::Identical
        } else {
            Verdict::Within { metric: "package parts".into(), value: 0.0 }
        }
    }
}

fn looks_like_xml(name: &str, data: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".xml") || lower.ends_with(".rels") {
        return true;
    }
    let body = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(data);
    body.trim_ascii_start().starts_with(b"<?xml")
}

fn compare_part(name: &str, a: &[u8], b: &[u8]) -> PartStatus {
    let bytes = compare_bytes(a, b);
    if bytes.equal {
        return PartStatus::Identical;
    }
    // A part that does not parse as UTF-8 XML falls through to the byte comparison.
    if looks_like_xml(name, a)
        && looks_like_xml(name, b)
        && let (Ok(ta), Ok(tb)) = (std::str::from_utf8(a), std::str::from_utf8(b))
        && let Ok(res) = compare_xml(ta.trim_start_matches('\u{feff}'), tb.trim_start_matches('\u{feff}'))
    {
        return match res {
            None => PartStatus::Equivalent,
            Some(path) => PartStatus::XmlDiffers { path },
        };
    }
    PartStatus::BytesDiffer { first_diff: bytes.first_diff.unwrap_or(0), len_a: bytes.len_a, len_b: bytes.len_b }
}

/// Compare two packages part by part.
pub fn compare_packages(a: &Package, b: &Package) -> PackageDiff {
    let mut diff = PackageDiff::default();
    for (name, da) in &a.parts {
        match b.parts.get(name) {
            None => diff.only_in_a.push(name.clone()),
            Some(db) => diff.common.push(PartDiff { name: name.clone(), status: compare_part(name, da, db) }),
        }
    }
    diff.only_in_b = b.parts.keys().filter(|n| !a.parts.contains_key(*n)).cloned().collect();
    diff
}

/// [`Package::open`] both inputs, then [`compare_packages`]. The two may be different container
/// kinds (a `.doc` against a converted `.docx` simply shares no part names).
pub fn compare_package_bytes(a: &[u8], b: &[u8]) -> Result<PackageDiff, CompareError> {
    Ok(compare_packages(&Package::open(a)?, &Package::open(b)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(parts: &[(&str, &str)]) -> Package {
        Package::from_parts(parts.iter().map(|(n, d)| (*n, d.as_bytes())))
    }

    #[test]
    fn reports_missing_and_extra_parts() {
        let a = pkg(&[("a.xml", "<r/>"), ("only-a.bin", "x"), ("c.bin", "1")]);
        let b = pkg(&[("a.xml", "<r/>"), ("only-b.bin", "y"), ("c.bin", "1")]);
        let d = compare_packages(&a, &b);
        assert_eq!(d.only_in_a, ["only-a.bin"]);
        assert_eq!(d.only_in_b, ["only-b.bin"]);
        assert_eq!(d.common.len(), 2);
        assert!(d.common.iter().all(|p| p.status == PartStatus::Identical));
        assert!(!d.is_equal());
        assert_eq!(d.difference_count(), 2);
        assert!(!d.verdict().passed());
    }

    #[test]
    fn identical_packages_are_identical() {
        let a = pkg(&[("x.bin", "abc"), ("y.xml", "<a b='1'/>")]);
        let d = compare_packages(&a, &a.clone());
        assert!(d.is_identical());
        assert_eq!(d.verdict(), Verdict::Identical);
    }

    #[test]
    fn xml_parts_compare_canonically() {
        let a = pkg(&[("word/document.xml", r#"<?xml version="1.0"?><w:d xmlns:w="urn:w" a="1" b="2"><w:p> </w:p></w:d>"#)]);
        let b = pkg(&[("word/document.xml", r#"<x:d b="2" a="1" xmlns:x="urn:w"><x:p/></x:d>"#)]);
        let d = compare_packages(&a, &b);
        assert_eq!(d.common[0].status, PartStatus::Equivalent);
        assert!(d.is_equal());
        assert!(!d.is_identical());
        assert!(matches!(d.verdict(), Verdict::Within { .. }));
    }

    #[test]
    fn xml_difference_reports_the_path() {
        let a = pkg(&[("p.xml", r#"<r><i id="1"/><i id="2"/></r>"#)]);
        let b = pkg(&[("p.xml", r#"<r><i id="1"/><i id="3"/></r>"#)]);
        let d = compare_packages(&a, &b);
        assert_eq!(d.common[0].status, PartStatus::XmlDiffers { path: "/r/i[2]/@id".into() });
        assert_eq!(d.differing().count(), 1);
    }

    #[test]
    fn xml_is_detected_by_prologue_and_rels_extension() {
        let a = pkg(&[("_rels/.rels", "<R b='1' a='2'/>"), ("data.dat", "<?xml version='1.0'?><R a='2' b='1'/>")]);
        let b = pkg(&[("_rels/.rels", "<R a='2' b='1'/>"), ("data.dat", "<?xml version='1.0'?><R b='1' a='2'/>")]);
        let d = compare_packages(&a, &b);
        assert!(d.common.iter().all(|p| p.status == PartStatus::Equivalent), "{d:?}");
    }

    #[test]
    fn non_xml_parts_compare_bytes_and_unparseable_xml_falls_back() {
        let a = pkg(&[("m.bin", "abcd"), ("bad.xml", "<a><b></a>")]);
        let b = pkg(&[("m.bin", "abXd"), ("bad.xml", "<a><b></b></a>")]);
        let d = compare_packages(&a, &b);
        assert_eq!(d.common[0].status, PartStatus::BytesDiffer { first_diff: 8, len_a: 10, len_b: 14 }, "bad.xml");
        assert_eq!(d.common[1].status, PartStatus::BytesDiffer { first_diff: 2, len_a: 4, len_b: 4 }, "m.bin");
    }

    #[test]
    fn container_detection() {
        assert_eq!(Container::detect(&CFB_MAGIC), Some(Container::Cfb));
        assert_eq!(Container::detect(b"PK\x03\x04rest"), Some(Container::Zip));
        assert_eq!(Container::detect(b"PK\x05\x06"), Some(Container::Zip));
        assert_eq!(Container::detect(b"%PDF"), None);
        assert!(matches!(Package::open(b"%PDF-1.7"), Err(CompareError::Package(_))));
    }

    #[cfg(feature = "zip")]
    mod zip {
        use super::*;
        use craft_archive::{Method, ZipWriter};

        fn zip(parts: &[(&str, &str)]) -> Vec<u8> {
            let mut w = ZipWriter::in_memory();
            for (n, d) in parts {
                w.add(n, d.as_bytes(), Method::Deflate(6)).unwrap();
            }
            w.finish().unwrap().into_inner()
        }

        #[test]
        fn reads_a_zip_and_diffs_parts() {
            let a = zip(&[("[Content_Types].xml", "<T/>"), ("word/document.xml", r#"<d a="1" b="2"/>"#), ("word/media/i.png", "PNG1")]);
            let b = zip(&[
                ("[Content_Types].xml", "<T/>"),
                ("word/document.xml", r#"<d b="2" a="1"/>"#),
                ("word/media/i.png", "PNG2"),
                ("extra.txt", "e"),
            ]);
            let pa = Package::from_zip(&a).unwrap();
            assert_eq!(pa.names().collect::<Vec<_>>(), ["[Content_Types].xml", "word/document.xml", "word/media/i.png"]);
            let d = compare_package_bytes(&a, &b).unwrap();
            assert_eq!(d.only_in_b, ["extra.txt"]);
            assert!(d.only_in_a.is_empty());
            let by_name = |n: &str| d.common.iter().find(|p| p.name == n).unwrap().status.clone();
            assert_eq!(by_name("[Content_Types].xml"), PartStatus::Identical);
            assert_eq!(by_name("word/document.xml"), PartStatus::Equivalent);
            assert_eq!(by_name("word/media/i.png"), PartStatus::BytesDiffer { first_diff: 3, len_a: 4, len_b: 4 });
            assert_eq!(d.difference_count(), 2);
        }

        #[test]
        fn identical_zips_have_no_differences() {
            let a = zip(&[("a", "1"), ("b/c", "2")]);
            assert!(compare_package_bytes(&a, &a).unwrap().is_identical());
        }

        #[test]
        fn corrupt_zip_is_an_error() {
            let mut a = zip(&[("a", "1")]);
            a.truncate(a.len() - 10);
            assert!(matches!(compare_package_bytes(&a, &a), Err(CompareError::Package(_))));
        }
    }

    #[cfg(feature = "cfb")]
    mod cfb {
        use super::*;

        fn put32(b: &mut [u8], at: usize, v: u32) {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }

        fn dir(b: &mut [u8], at: usize, name: &str, kind: u8, links: [u32; 3], start: u32, size: u32) {
            for (i, u) in name.encode_utf16().enumerate() {
                b[at + 2 * i..at + 2 * i + 2].copy_from_slice(&u.to_le_bytes());
            }
            b[at + 64..at + 66].copy_from_slice(&((name.len() as u16 + 1) * 2).to_le_bytes());
            b[at + 66] = kind;
            b[at + 67] = 1;
            put32(b, at + 68, links[1]);
            put32(b, at + 72, links[2]);
            put32(b, at + 76, links[0]);
            put32(b, at + 116, start);
            put32(b, at + 120, size);
        }

        /// A version-3 CFB with two 4096-byte regular-sector streams, `A` and `B`.
        fn cfb(a: u8, b: u8) -> Vec<u8> {
            const FREE: u32 = 0xffff_ffff;
            const END: u32 = 0xffff_fffe;
            let mut f = vec![0u8; 19 * 512];
            f[..8].copy_from_slice(&CFB_MAGIC);
            f[24..26].copy_from_slice(&0x3eu16.to_le_bytes());
            f[26..28].copy_from_slice(&3u16.to_le_bytes());
            f[28..30].copy_from_slice(&0xfffeu16.to_le_bytes());
            f[30..32].copy_from_slice(&9u16.to_le_bytes());
            f[32..34].copy_from_slice(&6u16.to_le_bytes());
            put32(&mut f, 44, 1); // one FAT sector
            put32(&mut f, 48, 1); // directory at sector 1
            put32(&mut f, 56, 4096);
            put32(&mut f, 60, END);
            put32(&mut f, 68, END);
            for i in 0..109 {
                put32(&mut f, 76 + 4 * i, if i == 0 { 0 } else { FREE });
            }
            let fat = 512;
            for i in 0..128 {
                put32(&mut f, fat + 4 * i, FREE);
            }
            put32(&mut f, fat, 0xffff_fffd);
            put32(&mut f, fat + 4, END);
            for id in 2..18u32 {
                put32(&mut f, fat + 4 * id as usize, if id == 9 || id == 17 { END } else { id + 1 });
            }
            let d = 2 * 512;
            dir(&mut f, d, "Root Entry", 5, [1, FREE, FREE], END, 0);
            dir(&mut f, d + 128, "A", 2, [FREE, FREE, 2], 2, 4096);
            dir(&mut f, d + 256, "B", 2, [FREE, FREE, FREE], 10, 4096);
            f[3 * 512..11 * 512].fill(a);
            f[11 * 512..19 * 512].fill(b);
            f
        }

        #[test]
        fn reads_cfb_streams() {
            let p = Package::from_cfb(&cfb(1, 2)).unwrap();
            assert_eq!(p.names().collect::<Vec<_>>(), ["A", "B"]);
            assert_eq!(p.get("A").unwrap(), vec![1u8; 4096].as_slice());
            assert_eq!(p.get("B").unwrap(), vec![2u8; 4096].as_slice());
        }

        #[test]
        fn diffs_cfb_streams() {
            let d = compare_package_bytes(&cfb(1, 2), &cfb(1, 9)).unwrap();
            assert!(d.only_in_a.is_empty() && d.only_in_b.is_empty());
            assert_eq!(d.common[0].status, PartStatus::Identical);
            assert_eq!(d.common[1].status, PartStatus::BytesDiffer { first_diff: 0, len_a: 4096, len_b: 4096 });
            assert!(compare_package_bytes(&cfb(1, 2), &cfb(1, 2)).unwrap().is_identical());
        }

        #[test]
        fn truncated_cfb_is_an_error() {
            let mut f = cfb(1, 2);
            f.truncate(600);
            assert!(matches!(Package::from_cfb(&f), Err(CompareError::Package(_))));
        }
    }
}
