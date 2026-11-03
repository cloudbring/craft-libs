use craft_ooxml::dom::{self, Limits, Namespaces, Node, ReadMode};

#[test]
fn deckcraft_malformed_recovery() {
    let lenient = Limits { read_mode: ReadMode::Lenient, ..Limits::default() };
    for (source, expected) in [
        ("<root><child>partial", "partial"),
        ("<root>bare & text</root>", "bare & text"),
        ("<root>before &unknown; after</root>", "before  after"),
        ("<root a='first' a='second'>ok</root>", "ok"),
        ("<root><child>text</wrong></root>", "text"),
        ("<root>first</root><other>ignored</other>", "first"),
        ("leading<root>kept</root>", "kept"),
    ] {
        let root = dom::parse(source.as_bytes(), &lenient).unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(root.text(), expected);
        assert!(dom::parse(source.as_bytes(), &Limits::default()).is_err(), "{source}");
    }
    let bom = b"\xef\xbb\xbf<root/>";
    assert_eq!(dom::parse(bom, &lenient).unwrap().name, "root");
    assert_eq!(dom::parse(bom, &Limits::default()).unwrap().name, "root");
}

#[test]
fn uri_namespaces_follow_aliases_and_rebindings() {
    let limits = Limits { namespaces: Namespaces::Uri, ..Limits::default() };
    let bytes = br#"<x:document xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:rr="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><x:p rr:id="r1"><y:t xmlns:y="http://purl.oclc.org/ooxml/wordprocessingml/main">hi</y:t><x:t xmlns:x="urn:other"/></x:p></x:document>"#;
    let root = dom::parse(bytes, &limits).unwrap();
    assert_eq!(root.prefix.as_deref(), Some("w"));
    let paragraph = root.child("p").unwrap();
    assert_eq!(paragraph.attr_ns("r", "id"), Some("r1"));
    assert_eq!(paragraph.child("t").unwrap().prefix.as_deref(), Some("w"));
    assert_eq!(paragraph.children("t").nth(1).unwrap().prefix.as_deref(), Some("?"));
    assert!(root.to_xml().contains("xmlns:w="));
    let prefix = dom::parse(bytes, &Limits::default()).unwrap();
    assert_eq!(prefix.prefix.as_deref(), Some("x"));
    let choice = br#"<mc:Choice xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:pen="http://schemas.microsoft.com/office/word/2010/wordprocessingInk" Requires="pen missing"/>"#;
    assert_eq!(dom::parse(choice, &limits).unwrap().attr("Requires"), Some("wpi ?"));
}

#[test]
fn deckcraft_element_conveniences() {
    let mut root = dom::parse(br#"<a:root xmlns:a="urn:a" n=" 3.5 " huge="999999999999" toggle="on"><a:outer><a:leaf val="ok">A&amp;<a:leaf>B</a:leaf>C</a:leaf></a:outer><a:leaf>D</a:leaf></a:root>"#, &Limits::default()).unwrap();
    assert!(root.is("root"));
    assert!(root.has_attr("n"));
    root.set_attr("toggle", "false");
    root.set_attr("added", "yes");
    assert_eq!(root.attrs.iter().filter(|(key, _)| key == "toggle").count(), 1);
    assert_eq!(root.i64("n"), Some(3));
    assert_eq!(root.i32("huge"), Some(i32::MAX));
    assert_eq!(root.u32("n"), Some(3));
    assert_eq!(root.f64("n"), Some(3.5));
    assert_eq!(root.bool("toggle"), Some(false));
    assert_eq!(root.elements().count(), 2);
    for element in root.elements_mut() {
        element.set_attr("visited", "1");
    }
    assert_eq!(root.child("outer").unwrap().attr("visited"), Some("1"));
    assert_eq!(root.children_named("leaf").count(), 1);
    assert_eq!(root.path(&["outer", "leaf"]).unwrap().child_val("leaf"), None);
    assert_eq!(root.child("outer").unwrap().child_val("leaf"), Some("ok"));
    assert_eq!(root.find("leaf").unwrap().attr("val"), Some("ok"));
    let mut all = Vec::new();
    root.find_all("leaf", &mut all);
    assert_eq!(all.len(), 3);
    assert_eq!(root.text(), "A&BCD");
    assert!(matches!(root.children.first(), Some(Node::El(_))));
    assert!(root.to_xml().contains("A&amp;"));
    root.set_attr("quote", "it's <fine>");
    assert!(root.to_xml().contains("quote=\"it's &lt;fine&gt;\""));
}

#[test]
fn mutation_never_panics_in_either_mode() {
    let seed = b"<r xmlns:w='urn:word'><w:p a='1'>hello &amp; world</w:p></r>";
    for index in 0..2_000usize {
        let mut bytes = seed.to_vec();
        let position = (index * 7919) % (bytes.len() + 1);
        if index % 3 == 0 {
            bytes.insert(position, (index >> 3) as u8);
        } else if position < bytes.len() && index % 3 == 1 {
            bytes.remove(position);
        } else if position < bytes.len() {
            bytes[position] = (index >> 2) as u8;
        }
        for mode in [ReadMode::Strict, ReadMode::Lenient] {
            let limits = Limits { read_mode: mode, max_total_bytes: 256, ..Limits::default() };
            let _ = dom::parse(&bytes, &limits);
        }
    }
}
