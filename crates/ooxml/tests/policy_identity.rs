use std::io::{Cursor, Write};

use craft_ooxml::opc::{ContentTypesPolicy, Method, MethodRule, PackageWriter, PartOrder, RelsPlacement, WritePolicy};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const PARTS: [(&str, &str, &[u8]); 3] = [
    ("ppt/presentation.xml", "application/vnd.test.presentation+xml", b"<presentation/>"),
    ("ppt/slides/slide1.xml", "application/vnd.test.slide+xml", b"<slide/>"),
    ("ppt/media/image1.png", "image/png", b"\x89PNG\r\n"),
];
const TYPES: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.test.presentation+xml\"/><Override PartName=\"/ppt/slides/slide1.xml\" ContentType=\"application/vnd.test.slide+xml\"/></Types>";
const WORD_PARTS: [(&str, &str, &[u8]); 3] = [
    ("word/document.xml", "application/vnd.test.document+xml", b"<document/>"),
    ("word/media/image1.png", "image/png", b"\x89PNG\r\n"),
    ("docProps/core.xml", "application/vnd.test.core+xml", b"<core/>"),
];
const WORD_TYPES: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.test.document+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.test.core+xml\"/></Types>";
const GRID_PARTS: [(&str, &str, &[u8]); 3] = [
    ("docProps/core.xml", "application/vnd.test.core+xml", b"<core/>"),
    ("xl/workbook.xml", "application/vnd.test.workbook+xml", b"<workbook/>"),
    ("xl/worksheets/sheet1.xml", "application/vnd.test.sheet+xml", b"<sheet/>"),
];
const GRID_TYPES: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.test.core+xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.test.workbook+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.test.sheet+xml\"/></Types>";

fn shared(policy: WritePolicy, entries: &[(&str, &str, &[u8])]) -> Vec<u8> {
    let mut writer = PackageWriter::with_policy(policy);
    for (name, kind, data) in entries {
        writer.add_part(name, kind, data.to_vec()).unwrap();
    }
    writer.finish().unwrap()
}

// Vendored from deckcraft/crates/pptx/src/opc.rs PackageOut::finish, specialized to three parts.
fn deckcraft_writer() -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("[Content_Types].xml", deflate).unwrap();
    zip.write_all(TYPES).unwrap();
    for (name, _, bytes) in PARTS {
        let options = if name.ends_with(".png") { stored } else { deflate };
        zip.start_file(name, options.large_file(bytes.len() as u64 >= u32::MAX as u64 / 2)).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

// Vendored from wordcraft/crates/docx/src/package.rs zip_entries; content types are caller-supplied.
fn wordcraft_writer() -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).compression_level(Some(6));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in std::iter::once(("[Content_Types].xml", WORD_TYPES)).chain(WORD_PARTS.iter().map(|(name, _, bytes)| (*name, *bytes))) {
        let lower = name.to_ascii_lowercase();
        let options = if [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|ext| lower.ends_with(ext)) { stored } else { deflate };
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

// Vendored from gridcraft/crates/xlsx/src/package.rs ZipOut::add, in write.rs ranked order.
fn gridcraft_writer() -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let entries = [GRID_PARTS[1], GRID_PARTS[2], GRID_PARTS[0]];
    for (name, bytes) in std::iter::once(("[Content_Types].xml", GRID_TYPES)).chain(entries.iter().map(|(name, _, bytes)| (*name, *bytes))) {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(zip::DateTime::default())
            .large_file(bytes.len() as u64 >= u32::MAX as u64);
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

// Vendored from designcraft/crates/pptx/src/package.rs write, specialized to its 2 XML + media phases.
fn designcraft_writer() -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("[Content_Types].xml", deflate).unwrap();
    zip.write_all(TYPES).unwrap();
    for (name, _, bytes) in PARTS {
        let options = if name.starts_with("ppt/media/") && !name.ends_with(".svg") { stored } else { deflate };
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn deckcraft_preset_is_byte_identical() {
    let actual = shared(WritePolicy::deckcraft(), &PARTS);
    let mut archive = zip::ZipArchive::new(Cursor::new(&actual)).unwrap();
    let mut content_types = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("[Content_Types].xml").unwrap(), &mut content_types).unwrap();
    assert_eq!(String::from_utf8_lossy(&content_types), String::from_utf8_lossy(TYPES));
    assert!(actual == deckcraft_writer(), "ZIP bytes diverged");
}

#[test]
fn wordcraft_preset_is_byte_identical() {
    let mut policy = WritePolicy::wordcraft();
    policy.content_types = ContentTypesPolicy::Preserve(WORD_TYPES.to_vec());
    assert_eq!(shared(policy, &WORD_PARTS), wordcraft_writer());
}

#[test]
fn gridcraft_preset_is_byte_identical() {
    let mut policy = WritePolicy::gridcraft();
    policy.content_types = ContentTypesPolicy::Preserve(GRID_TYPES.to_vec());
    assert_eq!(shared(policy, &GRID_PARTS), gridcraft_writer());
}

#[test]
fn designcraft_preset_is_byte_identical() {
    let mut policy = WritePolicy::designcraft();
    policy.content_types = ContentTypesPolicy::Preserve(TYPES.to_vec());
    assert_eq!(shared(policy, &PARTS), designcraft_writer());
}

#[test]
fn explicit_order_method_and_relationship_placement() {
    let mut policy = WritePolicy {
        part_order: PartOrder::Explicit(vec!["ppt/media/*.png".into(), "ppt/presentation.*".into()]),
        method: MethodRule::Custom(Box::new(|name| if name.ends_with(".png") { Method::Store } else { Method::Deflate(Some(6)) })),
        rels_placement: RelsPlacement::BeforeParts,
        ..WritePolicy::default()
    };
    policy.content_types = ContentTypesPolicy::Preserve(TYPES.to_vec());
    let mut writer = PackageWriter::with_policy(policy);
    for (name, kind, bytes) in PARTS {
        writer.add_part(name, kind, bytes.to_vec()).unwrap();
    }
    writer.add_relationship("", "rId1", "urn:test", "ppt/presentation.xml", craft_ooxml::opc::TargetMode::Internal).unwrap();
    let bytes = writer.finish().unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let names: Vec<_> = (0..archive.len()).map(|index| archive.by_index(index).unwrap().name().unwrap().into_owned()).collect();
    assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "ppt/media/image1.png", "ppt/presentation.xml", "ppt/slides/slide1.xml"]);
    assert_eq!(archive.by_name("ppt/media/image1.png").unwrap().compression(), CompressionMethod::Stored);
}

#[test]
fn insertion_preserves_part_spelling() {
    let mut policy = WritePolicy::wordcraft();
    policy.content_types = ContentTypesPolicy::Preserve(TYPES.to_vec());
    let bytes = shared(policy, &[("docProps/core.xml", "application/xml", b"<core/>")]);
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert!(archive.by_name("docProps/core.xml").is_ok());
}
