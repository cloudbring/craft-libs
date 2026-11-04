use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use craft_ooxml::dom::ReadMode;
use craft_ooxml::opc::{LazyLimits, LazyPackage, OpcError, OpcLimits, Package, Tolerance, parse_relationships, resolve_target_clamped};

const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#;
const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn rels(body: &str) -> String {
    format!(r#"<?xml version="1.0"?><Relationships xmlns="{RELS_NS}">{body}</Relationships>"#)
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).expect("start");
        writer.write_all(data).expect("write");
    }
    writer.finish().expect("finish").into_inner()
}

fn lenient() -> Tolerance {
    Tolerance::lenient()
}

#[test]
fn from_parts_builds_package_and_enforces_strictness() {
    let parts = BTreeMap::from([
        ("[Content_Types].xml".to_string(), CT.as_bytes().to_vec()),
        ("_rels/.rels".to_string(), rels(r#"<Relationship Id="r1" Type="t" Target="Word/Doc.xml"/>"#).into_bytes()),
        ("Word/Doc.xml".to_string(), b"<d/>".to_vec()),
    ]);
    let package = Package::from_parts(parts, &OpcLimits::default(), ReadMode::Strict).expect("parts");
    assert_eq!(package.part("word/doc.xml"), Some(&b"<d/>"[..]));
    assert_eq!(package.rels_of("").expect("rels").by_id("r1").expect("r1").target, "Word/Doc.xml");

    let no_ct = BTreeMap::from([("a.xml".to_string(), b"<a/>".to_vec())]);
    assert!(Package::from_parts(no_ct.clone(), &OpcLimits::default(), ReadMode::Strict).is_err());
    let package = Package::from_parts(no_ct, &OpcLimits::default(), ReadMode::Lenient).expect("lenient");
    assert_eq!(package.warnings().len(), 1);
}

#[test]
fn names_and_canonical_keep_original_case() {
    let bytes = zip_of(&[("[Content_Types].xml", CT.as_bytes()), ("Word/Doc.xml", b"<d/>")]);
    let package = Package::open(&bytes, &OpcLimits::default()).expect("open");
    let names: Vec<_> = package.part_names().collect();
    assert_eq!(names, ["[Content_Types].xml", "Word/Doc.xml"]);
    assert_eq!(package.canonical("/WORD/DOC.XML"), Some("Word/Doc.xml"));
    assert_eq!(package.canonical("nope.xml"), None);
}

#[test]
fn case_duplicates_merge_toward_relationship_spelling() {
    let bytes = zip_of(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", rels(r#"<Relationship Id="r1" Type="t" Target="word/Document.xml"/>"#).as_bytes()),
        ("word/document.xml", b"<lower/>"),
        ("word/Document.xml", b"<named/>"),
    ]);
    assert!(Package::open(&bytes, &OpcLimits::default()).is_err());
    let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("open");
    assert_eq!(package.canonical("word/document.xml"), Some("word/Document.xml"));
    assert_eq!(package.part("word/document.xml"), Some(&b"<named/>"[..]));
    assert_eq!(package.part_names().filter(|name| name.starts_with("word/")).count(), 1);
    assert!(package.warnings().iter().any(|w| w.contains("case-duplicate")));
}

#[test]
fn missing_content_types_is_opt_in() {
    let bytes = zip_of(&[("a.xml", b"<a/>")]);
    assert!(Package::open(&bytes, &OpcLimits::default()).is_err());
    let tolerance = Tolerance { missing_content_types: true, ..Tolerance::default() };
    let package = Package::open_with(&bytes, &OpcLimits::default(), &tolerance).expect("open");
    assert!(package.content_types.defaults.is_empty() && package.content_types.overrides.is_empty());
    assert!(package.warnings().iter().any(|w| w.contains("Content_Types")));
}

#[test]
fn malformed_rels_degrade_with_warnings() {
    let bytes = zip_of(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", b"<Wrong/>"),
        ("a.xml", b"<a/>"),
        (
            "_rels/a.xml.rels",
            rels(r#"<Relationship Id="x" Type="t" Target="b.xml"/><Relationship Id="x" Type="t" Target="c.xml"/><Relationship Id="y" Type="t"/>"#)
                .as_bytes(),
        ),
    ]);
    assert!(Package::open(&bytes, &OpcLimits::default()).is_err());
    let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("open");
    assert!(package.rels_of("").expect("root").0.is_empty());
    let partial = package.rels_of("a.xml").expect("a rels");
    assert_eq!(partial.0.len(), 1);
    assert_eq!(partial.by_id("x").expect("x").target, "b.xml");
    assert_eq!(package.warnings().len(), 3);
}

#[test]
fn corrupt_entry_is_skipped_but_counts_against_budget() {
    let payload = vec![b'a'; 4000];
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [("[Content_Types].xml", CT.as_bytes()), ("bad.xml", &payload[..]), ("good.xml", b"<g/>")] {
        writer.start_file(name, deflated).expect("start");
        writer.write_all(data).expect("write");
    }
    let mut bytes = writer.finish().expect("finish").into_inner();
    // Corrupt the compressed payload of bad.xml (first deflate bytes follow its local header).
    let at = bytes.windows(7).position(|w| w == b"bad.xml").expect("name") + 7;
    for byte in &mut bytes[at..at + 6] {
        *byte = 0xff;
    }
    assert!(Package::open(&bytes, &OpcLimits::default()).is_err());
    let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("open");
    assert!(package.part("bad.xml").is_none());
    assert_eq!(package.part("good.xml"), Some(&b"<g/>"[..]));
    assert!(package.warnings().iter().any(|w| w.contains("bad.xml")));

    // The skipped entry's declared 4000 bytes still count: a budget it would bust fails the open.
    let mut tight = OpcLimits::default();
    tight.zip.max_total_bytes = 3000 + CT.len() as u64;
    assert!(matches!(Package::open_with(&bytes, &tight, &lenient()), Err(OpcError::Zip(_))));
}

#[test]
fn shared_parts_outlive_the_package() {
    let bytes = zip_of(&[("[Content_Types].xml", CT.as_bytes()), ("a.xml", b"<a/>")]);
    let package = Package::open(&bytes, &OpcLimits::default()).expect("open");
    let shared = package.part_shared("A.XML").expect("part");
    assert!(std::sync::Arc::ptr_eq(&shared, &package.part_shared("a.xml").expect("part")));
    drop(package);
    assert_eq!(&shared[..], b"<a/>");
}

#[test]
fn lazy_reads_non_opc_zip_with_budget() {
    let bytes = zip_of(&[("mimetype", b"application/vnd.oasis.opendocument.spreadsheet"), ("content.xml", &[b'x'; 100]), ("META-INF/", b"")]);
    let mut lazy = LazyPackage::open(bytes.clone(), &OpcLimits::default()).expect("open");
    assert_eq!(lazy.part_names().collect::<Vec<_>>(), ["content.xml", "mimetype"]);
    assert_eq!(lazy.declared_size("content.xml"), Some(100));
    assert_eq!(lazy.read("content.xml", 1000).expect("read").len(), 100);
    assert_eq!(lazy.read_optional("absent", 10).expect("opt"), None);
    assert!(lazy.read("absent", 10).is_err());
    assert!(matches!(lazy.read("content.xml", 50), Err(OpcError::Limit(_))));

    let mut limits = OpcLimits::default();
    limits.zip.max_total_bytes = 150;
    let mut lazy = LazyPackage::open(bytes, &limits).expect("open");
    lazy.read("content.xml", 1000).expect("first");
    assert!(matches!(lazy.read("content.xml", 1000), Err(OpcError::Limit("package bytes"))));
}

#[test]
fn lazy_parses_content_types_and_isolates_damage() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [("[Content_Types].xml", CT.as_bytes()), ("bad.xml", &[b'a'; 4000][..]), ("good.xml", b"<g/>")] {
        writer.start_file(name, deflated).expect("start");
        writer.write_all(data).expect("write");
    }
    let mut bytes = writer.finish().expect("finish").into_inner();
    let at = bytes.windows(7).position(|w| w == b"bad.xml").expect("name") + 7;
    for byte in &mut bytes[at..at + 6] {
        *byte = 0xff;
    }
    let mut lazy = LazyPackage::open(bytes, &OpcLimits::default()).expect("open");
    assert_eq!(lazy.content_types.of("x.xml"), Some("application/xml"));
    assert!(lazy.read("bad.xml", 10_000).is_err());
    assert_eq!(lazy.read("good.xml", 100).expect("good"), b"<g/>");
}

#[test]
fn lazy_ratio_guard_applies_per_read() {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(9));
    writer.start_file("bomb.bin", deflated).expect("start");
    writer.write_all(&vec![0u8; 8 * 1024 * 1024]).expect("write");
    let bytes = writer.finish().expect("finish").into_inner();
    let mut lazy = LazyPackage::open(bytes, &OpcLimits::default()).expect("open");
    assert!(matches!(lazy.read("bomb.bin", u64::MAX), Err(OpcError::Limit("expansion ratio"))));
}

fn corrupt_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [("[Content_Types].xml", CT.as_bytes()), ("bad.xml", &[b'a'; 4000][..]), ("good.xml", b"<g/>")] {
        writer.start_file(name, deflated).expect("start");
        writer.write_all(data).expect("write");
    }
    let mut bytes = writer.finish().expect("finish").into_inner();
    let at = bytes.windows(7).position(|w| w == b"bad.xml").expect("name") + 7;
    for byte in &mut bytes[at..at + 6] {
        *byte = 0xff;
    }
    bytes
}

#[test]
fn shared_bytes_are_arc_vec_and_from_parts_moves_arcs() {
    let shared = std::sync::Arc::new(b"<d/>".to_vec());
    let parts =
        BTreeMap::from([("[Content_Types].xml".to_string(), std::sync::Arc::new(CT.as_bytes().to_vec())), ("d.xml".to_string(), shared.clone())]);
    let package = Package::from_parts(parts, &OpcLimits::default(), ReadMode::Strict).expect("parts");
    assert!(std::sync::Arc::ptr_eq(&package.part_shared("D.xml").expect("part"), &shared));
}

#[test]
fn lenient_part_names_keep_raw_name() {
    let bytes = zip_of(&[("[Content_Types].xml", CT.as_bytes()), ("ab:c?.xml", b"<x/>"), ("ok.xml", b"<o/>")]);
    assert!(Package::open(&bytes, &OpcLimits::default()).is_err());
    let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("open");
    assert_eq!(package.part("ab:c?.xml"), Some(&b"<x/>"[..]));
    assert_eq!(package.part("ok.xml"), Some(&b"<o/>"[..]));
    assert!(package.warnings().iter().any(|w| w.contains("unsafe part name")));
    let parts = BTreeMap::from([("[Content_Types].xml".to_string(), CT.as_bytes().to_vec()), ("../x.xml".to_string(), b"<x/>".to_vec())]);
    assert!(Package::from_parts(parts.clone(), &OpcLimits::default(), ReadMode::Strict).is_err());
    assert!(Package::from_parts(parts, &OpcLimits::default(), ReadMode::Lenient).expect("lenient").part("../x.xml").is_some());
}

#[test]
fn lenient_content_types_warn_instead_of_failing() {
    let bad_override = r#"<Types xmlns="x"><Default Extension="xml" ContentType="a/b"/><Override PartName="/a/../b.xml" ContentType="c/d"/></Types>"#;
    for ct in [bad_override, "<not xml", "<Wrong/>"] {
        let bytes = zip_of(&[("[Content_Types].xml", ct.as_bytes())]);
        assert!(Package::open(&bytes, &OpcLimits::default()).is_err(), "{ct}");
        let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("lenient");
        assert!(!package.warnings().is_empty());
    }
    let bytes = zip_of(&[("[Content_Types].xml", bad_override.as_bytes())]);
    let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("lenient");
    assert_eq!(package.content_types.of("z.xml"), Some("a/b"));
    assert!(package.content_types.overrides.is_empty());
}

#[test]
fn clamp_targets_stop_at_root() {
    assert_eq!(resolve_target_clamped("word", "../../../x"), "x");
    assert_eq!(resolve_target_clamped("word", "media/a%20b.png#f"), "word/media/a b.png");
    assert_eq!(resolve_target_clamped("word", "/abs/y"), "abs/y");
    let bytes = zip_of(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("word/document.xml", b"<d/>"),
        ("word/_rels/document.xml.rels", rels(r#"<Relationship Id="r" Type="t" Target="../../../x"/>"#).as_bytes()),
    ]);
    assert!(Package::open(&bytes, &OpcLimits::default()).is_err());
    let package = Package::open_with(&bytes, &OpcLimits::default(), &lenient()).expect("open");
    assert_eq!(package.rels_of("word/document.xml").expect("rels").by_id("r").expect("r").target, "x");
}

#[test]
fn parse_relationships_standalone() {
    let xml = rels(
        r#"<Relationship Id="a" Type="t" Target="../../x"/><Relationship Id="b" Type="t" Target="http://e/" TargetMode="External"/><Relationship Id="a" Type="t" Target="dup"/>"#,
    );
    let (found, warnings) = parse_relationships(xml.as_bytes(), "word/document.xml", &lenient());
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].target, "x");
    assert_eq!(found[1].target, "http://e/");
    assert_eq!(warnings.len(), 1);
    let (found, warnings) = parse_relationships(b"garbage", "", &Tolerance::default());
    assert!(found.is_empty() && !warnings.is_empty());
}

#[test]
fn lazy_rels_of_and_find() {
    let bytes = zip_of(&[
        ("Word/Document.xml", b"<d/>"),
        ("Word/_rels/Document.xml.rels", rels(r#"<Relationship Id="r" Type="t" Target="../../s.xml"/>"#).as_bytes()),
    ]);
    let mut lazy = LazyPackage::open(&bytes[..], &OpcLimits::default()).expect("borrowed open");
    assert_eq!(lazy.find("/word/document.XML"), Some("Word/Document.xml"));
    assert_eq!(lazy.find("nope"), None);
    let found = lazy.rels_of("word/document.xml", 10_000).expect("rels");
    assert_eq!(found[0].target, "s.xml");
    assert!(lazy.rels_of("absent.xml", 10).expect("none").is_empty());
}

#[test]
fn lazy_failed_reads_charge_budget_and_report_corrupt() {
    let bytes = corrupt_zip();
    let mut plain = LazyPackage::open(bytes.clone(), &OpcLimits::default()).expect("open");
    let before = plain.total_read();
    assert!(matches!(plain.read_optional("bad.xml", 10_000), Err(OpcError::Corrupt(_))));
    assert!(plain.total_read() - before < 10_000);

    let mut strict = LazyPackage::open_with(bytes, &OpcLimits::default(), LazyLimits { charge_failed_reads_full_cap: true }).expect("open");
    let before = strict.total_read();
    assert!(matches!(strict.read_optional("bad.xml", 10_000), Err(OpcError::Corrupt(_))));
    assert_eq!(strict.total_read() - before, 10_000);
    assert!(matches!(strict.read("bad.xml", 10_000), Err(OpcError::Corrupt(_))));
    assert_eq!(strict.remaining_budget(), 0);
    assert!(matches!(strict.read("good.xml", 100), Err(OpcError::Limit("package bytes"))));
}

#[test]
fn lazy_charge_consumes_budget() {
    let bytes = zip_of(&[("a.xml", b"<a/>")]);
    let mut limits = OpcLimits::default();
    limits.zip.max_total_bytes = 100;
    let mut lazy = LazyPackage::open(bytes, &limits).expect("open");
    lazy.charge(60).expect("charge");
    assert_eq!(lazy.remaining_budget(), 40);
    assert!(matches!(lazy.charge(41), Err(OpcError::Limit("package bytes"))));
    assert_eq!(lazy.remaining_budget(), 0);
}
