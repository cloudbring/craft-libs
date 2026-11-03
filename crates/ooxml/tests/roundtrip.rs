use craft_ooxml::dom::{self, El, Limits, Node, XmlError};
use craft_ooxml::opc::{OpcLimits, Package, PackageWriter, TargetMode, resolve_target};
use craft_ooxml::write::XmlWriter;
use proptest::prelude::*;

fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            5 => (0x20u32..0xd7ff).prop_filter_map("XML character", char::from_u32),
            1 => prop::sample::select(vec!['\n', '\t', '\r', '&', '<', '>', '\'', '"']),
        ],
        0..30,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn tree() -> impl Strategy<Value = El> {
    let leaf = text().prop_map(|value| El {
        name: "t".into(),
        prefix: Some("x".into()),
        attrs: vec![("xml:space".into(), "preserve".into())],
        children: vec![Node::Text(value)],
    });
    leaf.prop_recursive(4, 32, 4, |inner| {
        prop::collection::vec(inner, 0..4).prop_map(|children| El {
            name: "branch".into(),
            prefix: Some("x".into()),
            attrs: vec![],
            children: children.into_iter().map(Node::El).collect(),
        })
    })
    .prop_map(|child| El {
        name: "root".into(),
        prefix: None,
        attrs: vec![("xmlns:x".into(), "urn:test".into())],
        children: vec![Node::El(child)],
    })
}

proptest! {
    #[test]
    fn escapes_roundtrip(value in text()) {
        let mut writer = XmlWriter::new();
        writer.start("root").attr("value", &value).text(&value).end();
        let root = dom::parse(&writer.finish(), &Limits::default())?;
        prop_assert_eq!(root.attr("value"), Some(value.as_str()));
        prop_assert_eq!(root.text(), value);
    }

    #[test]
    fn tree_roundtrip(root in tree()) {
        let mut writer = XmlWriter::new();
        writer.declaration().element(&root);
        let first = dom::parse(&writer.finish(), &Limits::default())?;
        let mut writer = XmlWriter::new();
        writer.element(&first);
        let second = dom::parse(&writer.finish(), &Limits::default())?;
        prop_assert_eq!(first, second);
    }
}

#[test]
fn limits_reject_before_growth() {
    let limits = Limits { max_depth: 1, ..Limits::default() };
    assert!(matches!(dom::parse(b"<a><b/></a>", &limits), Err(XmlError::Limit("depth"))));
    let limits = Limits { max_nodes: 1, ..Limits::default() };
    assert!(matches!(dom::parse(b"<a><b/></a>", &limits), Err(XmlError::Limit("nodes"))));
    let limits = Limits { max_text_bytes: 1, ..Limits::default() };
    assert!(matches!(dom::parse(b"<a>&amp;&amp;</a>", &limits), Err(XmlError::Limit("text bytes"))));
    let limits = Limits { max_attr_bytes: 1, ..Limits::default() };
    assert!(matches!(dom::parse(b"<a key='long'/>", &limits), Err(XmlError::Limit("attribute bytes"))));
    let limits = Limits { max_total_bytes: 3, ..Limits::default() };
    assert!(matches!(dom::parse(b"<a/>", &limits), Err(XmlError::Limit("input bytes"))));
    assert!(dom::parse(b"<a><b></a>", &Limits::default()).is_err());
    assert!(dom::parse(b"<!DOCTYPE a><a/>", &Limits::default()).is_err());
}

#[test]
fn local_names_and_attribute_precedence() -> Result<(), Box<dyn std::error::Error>> {
    let root =
        dom::parse(b"<p:r xmlns:p='urn:p' xmlns:r='urn:r' id='plain' r:id='related'><p:t xml:space='preserve'>hi</p:t></p:r>", &Limits::default())?;
    assert_eq!(root.name, "r");
    assert_eq!(root.prefix.as_deref(), Some("p"));
    assert_eq!(root.attr("id"), Some("plain"));
    assert_eq!(root.attr_ns("r", "id"), Some("related"));
    assert_eq!(root.child("t").map(El::text).as_deref(), Some("hi"));
    Ok(())
}

#[test]
fn utf16_input_and_bad_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = vec![0xff, 0xfe];
    for unit in "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a>✓</a>".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(dom::parse(&bytes, &Limits::default())?.text(), "✓");
    bytes.push(0);
    assert!(dom::parse(&bytes, &Limits::default()).is_err());
    Ok(())
}

#[test]
fn package_roundtrip_and_reproducibility() -> Result<(), Box<dyn std::error::Error>> {
    fn make() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut writer = PackageWriter::new();
        writer.add_part("ppt/presentation.xml", "application/presentation+xml", b"<p/>".to_vec())?;
        writer.add_part("ppt/slides/slide1.xml", "application/slide+xml", b"<s/>".to_vec())?;
        writer.add_part("ppt/media/image.png", "image/png", vec![0, 1, 2])?;
        writer.add_relationship("", "rId1", "officeDocument", "ppt/presentation.xml", TargetMode::Internal)?;
        writer.add_relationship("ppt/presentation.xml", "rId2", "slide", "slides/slide1.xml", TargetMode::Internal)?;
        writer.add_relationship("ppt/slides/slide1.xml", "rId3", "link", "https://example.org/#here", TargetMode::External)?;
        Ok(writer.finish()?)
    }
    let bytes = make()?;
    assert_eq!(bytes, make()?);
    let package = Package::open(&bytes, &OpcLimits::default())?;
    assert_eq!(package.part("/PPT/slides/SLIDE1.XML"), Some(b"<s/>".as_slice()));
    assert_eq!(package.content_types.of("ppt/presentation.xml"), Some("application/presentation+xml"));
    assert_eq!(package.content_types.of("ppt/media/image.png"), Some("image/png"));
    assert_eq!(package.rels_of("").and_then(|r| r.by_id("rId1")).map(|r| r.target.as_str()), Some("ppt/presentation.xml"));
    assert_eq!(package.rels_of("ppt/presentation.xml").and_then(|r| r.by_type("slide")).map(|r| r.target.as_str()), Some("ppt/slides/slide1.xml"));
    assert_eq!(package.rels_of("ppt/slides/slide1.xml").and_then(|r| r.by_id("rId3")).map(|r| r.mode), Some(TargetMode::External));
    let limits = OpcLimits { zip: craft_archive::ZipLimits { max_entries: 1, ..OpcLimits::default().zip }, ..OpcLimits::default() };
    assert!(Package::open(&bytes, &limits).is_err());
    Ok(())
}

#[test]
fn relative_targets_cannot_escape_package() {
    assert_eq!(resolve_target("ppt/slides/one.xml", "../media/a%23b.png#fragment").ok().as_deref(), Some("ppt/media/a#b.png"));
    assert_eq!(resolve_target("ppt/slides/one.xml", "#bookmark").ok().as_deref(), Some("ppt/slides/one.xml"));
    assert!(resolve_target("a.xml", "../escape.xml").is_err());
    assert!(resolve_target("a.xml", "%2e%2e/escape.xml").is_err());
    assert!(resolve_target("a.xml", "//example.org/a.xml").is_err());
    assert!(resolve_target("a.xml", "https://example.org/a.xml").is_err());
}

#[test]
fn content_type_conflicts_become_overrides() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = PackageWriter::new();
    writer.add_part("a.xml", "application/one", b"<a/>".to_vec())?;
    writer.add_part("b.xml", "application/two", b"<b/>".to_vec())?;
    let package = Package::open(&writer.finish()?, &OpcLimits::default())?;
    assert_eq!(package.content_types.of("a.xml"), Some("application/one"));
    assert_eq!(package.content_types.of("b.xml"), Some("application/two"));
    Ok(())
}

#[test]
fn writer_preserves_attribute_whitespace_and_strips_controls() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = XmlWriter::new();
    writer.start("t").space_preserve().attr("a", "\t\n\r\"'\u{1}").text("\r<>\u{2}").end();
    let bytes = writer.finish();
    let xml = std::str::from_utf8(&bytes)?;
    assert!(xml.contains("&#9;&#10;&#13;&quot;&apos;"));
    let parsed = dom::parse(&bytes, &Limits::default())?;
    assert_eq!(parsed.attr("a"), Some("\t\n\r\"'"));
    assert_eq!(parsed.text(), "\r<>");
    Ok(())
}

#[test]
fn rejects_unsafe_parts_and_duplicate_relationships() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = PackageWriter::new();
    assert!(writer.add_part("../escape", "image/png", vec![]).is_err());
    writer.add_part("a.xml", "application/xml", vec![])?;
    assert!(writer.add_part("A.XML", "application/xml", vec![]).is_err());
    writer.add_relationship("", "rId1", "officeDocument", "a.xml", TargetMode::Internal)?;
    assert!(writer.add_relationship("", "rId1", "officeDocument", "a.xml", TargetMode::Internal).is_err());
    assert!(writer.add_relationship("missing.xml", "rId2", "officeDocument", "a.xml", TargetMode::Internal).is_ok());
    assert!(writer.finish().is_err());
    Ok(())
}
