//! Bounded local-name XML tree. Provenance: deckcraft/crates/pptx/src/xml.rs,
//! wordcraft/crates/docx/src/xml.rs and gridcraft/crates/xlsx/src/xml.rs.

use std::borrow::Cow;
use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::write::XmlWriter;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadMode {
    #[default]
    Strict,
    Lenient,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Namespaces {
    #[default]
    Prefix,
    Uri,
}

/// Per-document caps. Text and attribute budgets count decoded UTF-8 bytes across the tree.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_text_bytes: usize,
    pub max_attr_bytes: usize,
    pub max_total_bytes: usize,
    pub read_mode: ReadMode,
    pub namespaces: Namespaces,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_nodes: 1_000_000,
            max_text_bytes: 96 * 1024 * 1024,
            max_attr_bytes: 1024 * 1024,
            max_total_bytes: 96 * 1024 * 1024,
            read_mode: ReadMode::Strict,
            namespaces: Namespaces::Prefix,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum XmlError {
    #[error("XML limit exceeded: {0}")]
    Limit(&'static str),
    #[error("invalid XML: {0}")]
    Invalid(String),
}

/// Element names are local; the original prefix is retained for lossless serialization.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct El {
    pub name: String,
    pub prefix: Option<String>,
    /// Attribute keys keep their qualified spelling (including namespace declarations).
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    El(El),
    Text(String),
}

impl El {
    pub fn is(&self, name: &str) -> bool {
        self.name == local(name)
    }

    pub fn has_attr(&self, name: &str) -> bool {
        self.attr(name).is_some()
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        if let Some((_, previous)) = self.attrs.iter_mut().find(|(key, _)| key == name) {
            *previous = value.to_string();
        } else {
            self.attrs.push((name.to_string(), value.to_string()));
        }
    }

    pub fn i64(&self, name: &str) -> Option<i64> {
        let value = self.attr(name)?.trim();
        value
            .parse::<i64>()
            .ok()
            .or_else(|| value.parse::<f64>().ok().filter(|number| number.is_finite()).map(|number| number.clamp(-9.0e18, 9.0e18) as i64))
    }

    pub fn i32(&self, name: &str) -> Option<i32> {
        self.i64(name).map(|value| value.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }

    pub fn u32(&self, name: &str) -> Option<u32> {
        self.i64(name).map(|value| value.clamp(0, u32::MAX as i64) as u32)
    }

    pub fn f64(&self, name: &str) -> Option<f64> {
        self.attr(name)?.trim().parse::<f64>().ok().filter(|number| number.is_finite())
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.attr(name)?.trim() {
            "1" | "true" | "on" => Some(true),
            "0" | "false" | "off" => Some(false),
            _ => None,
        }
    }

    pub fn elements(&self) -> impl DoubleEndedIterator<Item = &El> {
        self.children.iter().filter_map(|node| match node {
            Node::El(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    pub fn elements_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut El> {
        self.children.iter_mut().filter_map(|node| match node {
            Node::El(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&El> {
        self.elements().find(|el| el.is(name))
    }

    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.children_named(name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.elements().filter(move |el| el.is(name))
    }

    pub fn path(&self, names: &[&str]) -> Option<&El> {
        let mut current = self;
        for name in names {
            current = current.child(name)?;
        }
        Some(current)
    }

    pub fn find(&self, name: &str) -> Option<&El> {
        let mut pending: Vec<_> = self.elements().rev().collect();
        while let Some(el) = pending.pop() {
            if el.is(name) {
                return Some(el);
            }
            pending.extend(el.elements().rev());
        }
        None
    }

    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a El>) {
        let mut pending: Vec<_> = self.elements().rev().collect();
        while let Some(el) = pending.pop() {
            if el.is(name) {
                out.push(el);
            }
            pending.extend(el.elements().rev());
        }
    }

    pub fn child_val(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|el| el.attr("val"))
    }

    pub fn to_xml(&self) -> String {
        let mut writer = XmlWriter::deckcraft();
        writer.element(self);
        String::from_utf8(writer.finish()).unwrap_or_default()
    }

    /// Plain key takes precedence over a prefixed key of the same local name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        let name = local(name);
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .or_else(|| self.attrs.iter().find(|(key, _)| !key.starts_with("xmlns") && local(key) == name))
            .map(|(_, value)| value.as_str())
    }

    pub fn attr_ns(&self, prefix: &str, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(key, _)| key.split_once(':').is_some_and(|(p, n)| p == prefix && n == name)).map(|(_, value)| value.as_str())
    }

    /// Concatenates text in document order, including descendants.
    pub fn text(&self) -> String {
        let mut result = String::new();
        let mut pending: Vec<_> = self.children.iter().rev().collect();
        while let Some(node) = pending.pop() {
            match node {
                Node::Text(text) => result.push_str(text),
                Node::El(el) => pending.extend(el.children.iter().rev()),
            }
        }
        result
    }
}

fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

// Provenance: wordcraft/crates/docx/src/xml.rs NAMESPACES, OTHER_NAMESPACES, STRICT.
const CANONICAL_URIS: &[(&str, &str)] = &[
    ("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"),
    ("r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships"),
    ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"),
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    ("c", "http://schemas.openxmlformats.org/drawingml/2006/chart"),
    ("pic", "http://schemas.openxmlformats.org/drawingml/2006/picture"),
    ("mc", "http://schemas.openxmlformats.org/markup-compatibility/2006"),
    ("w14", "http://schemas.microsoft.com/office/word/2010/wordml"),
    ("wpi", "http://schemas.microsoft.com/office/word/2010/wordprocessingInk"),
    ("w15", "http://schemas.microsoft.com/office/word/2012/wordml"),
    ("wp14", "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing"),
    ("wps", "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"),
    ("wpg", "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"),
    ("wpc", "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"),
    ("dgm", "http://schemas.openxmlformats.org/drawingml/2006/diagram"),
    ("dsp", "http://schemas.microsoft.com/office/drawing/2008/diagram"),
    ("m", "http://schemas.openxmlformats.org/officeDocument/2006/math"),
    ("v", "urn:schemas-microsoft-com:vml"),
    ("o", "urn:schemas-microsoft-com:office:office"),
    ("w10", "urn:schemas-microsoft-com:office:word"),
    ("cx", "http://schemas.microsoft.com/office/drawing/2014/chartex"),
    ("cx1", "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex"),
    ("cx2", "http://schemas.microsoft.com/office/drawing/2015/10/21/chartex"),
    ("cx3", "http://schemas.microsoft.com/office/drawing/2016/5/9/chartex"),
    ("cx4", "http://schemas.microsoft.com/office/drawing/2016/5/10/chartex"),
    ("cx5", "http://schemas.microsoft.com/office/drawing/2016/5/11/chartex"),
    ("cx6", "http://schemas.microsoft.com/office/drawing/2016/5/12/chartex"),
    ("cx7", "http://schemas.microsoft.com/office/drawing/2016/5/13/chartex"),
    ("cx8", "http://schemas.microsoft.com/office/drawing/2016/5/14/chartex"),
    ("b", "http://schemas.openxmlformats.org/officeDocument/2006/bibliography"),
    ("rel", "http://schemas.openxmlformats.org/package/2006/relationships"),
    ("ct", "http://schemas.openxmlformats.org/package/2006/content-types"),
    ("cp", "http://schemas.openxmlformats.org/package/2006/metadata/core-properties"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("xsi", "http://www.w3.org/2001/XMLSchema-instance"),
    ("xml", "http://www.w3.org/XML/1998/namespace"),
    ("ep", "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"),
    ("op", "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"),
    ("vt", "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"),
];
const STRICT_URIS: &[(&str, &str)] = &[
    ("w", "http://purl.oclc.org/ooxml/wordprocessingml/main"),
    ("r", "http://purl.oclc.org/ooxml/officeDocument/relationships"),
    ("wp", "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing"),
    ("a", "http://purl.oclc.org/ooxml/drawingml/main"),
    ("pic", "http://purl.oclc.org/ooxml/drawingml/picture"),
    ("m", "http://purl.oclc.org/ooxml/officeDocument/math"),
];

fn canonical(uri: &str) -> Option<&'static str> {
    CANONICAL_URIS.iter().chain(STRICT_URIS).find_map(|(prefix, known)| (*known == uri).then_some(*prefix))
}

fn requirement_prefix(uri: &str) -> &'static str {
    canonical(uri).unwrap_or(match uri {
        "http://schemas.microsoft.com/office/drawing/2010/main" => "a14",
        "http://schemas.microsoft.com/office/word/2015/wordml/symex" => "w16se",
        "http://schemas.microsoft.com/office/word/2016/wordml/cid" => "w16cid",
        "http://schemas.microsoft.com/office/word/2018/wordml" => "w16",
        "http://schemas.microsoft.com/office/word/2018/wordml/cex" => "w16cex",
        "http://schemas.microsoft.com/office/word/2020/wordml/sdtdatahash" => "w16sdtdh",
        _ => "?",
    })
}

fn canonicalize(el: &mut El, scopes: &[BTreeMap<String, String>], used: &mut BTreeMap<String, String>) {
    let resolve = |prefix: &str| {
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(prefix))
            .map(String::as_str)
            .or_else(|| (prefix == "xml").then_some("http://www.w3.org/XML/1998/namespace"))
    };
    let original = el.prefix.as_deref().unwrap_or("");
    if let Some(uri) = resolve(original) {
        let prefix = canonical(uri).unwrap_or("?");
        el.prefix = Some(prefix.into());
        if prefix != "?"
            && let Some((_, standard)) = CANONICAL_URIS.iter().find(|(name, _)| *name == prefix)
        {
            used.insert(prefix.into(), (*standard).into());
        }
    }
    el.attrs.retain(|(key, _)| key != "xmlns" && !key.starts_with("xmlns:"));
    for (key, _) in &mut el.attrs {
        if let Some((prefix, suffix)) = key.split_once(':')
            && let Some(uri) = resolve(prefix)
        {
            let normalized = canonical(uri).unwrap_or("?");
            *key = format!("{normalized}:{suffix}");
            if normalized != "?"
                && let Some((_, standard)) = CANONICAL_URIS.iter().find(|(name, _)| *name == normalized)
            {
                used.insert(normalized.into(), (*standard).into());
            }
        }
    }
    if el.prefix.as_deref() == Some("mc") && el.name == "Choice" {
        for (key, value) in &mut el.attrs {
            if key == "Requires" {
                let requirements: Vec<_> = value.split_whitespace().take(65).collect();
                *value = if requirements.len() > 64 || requirements.iter().any(|prefix| prefix.len() > 128) {
                    "?".into()
                } else {
                    requirements.iter().map(|prefix| resolve(prefix).map_or("?", requirement_prefix)).collect::<Vec<_>>().join(" ")
                };
            }
        }
    }
}

fn attach(stack: &mut [El], root: &mut Option<El>, el: El) -> Result<(), XmlError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(Node::El(el));
    } else if root.replace(el).is_some() {
        return Err(XmlError::Invalid("multiple roots".into()));
    }
    Ok(())
}

fn add_text(stack: &mut [El], value: &str, used: &mut usize, limit: usize) -> Result<(), XmlError> {
    *used = used.checked_add(value.len()).filter(|n| *n <= limit).ok_or(XmlError::Limit("text bytes"))?;
    if let Some(el) = stack.last_mut() {
        if let Some(Node::Text(last)) = el.children.last_mut() {
            last.push_str(value);
        } else if !value.is_empty() {
            el.children.push(Node::Text(value.to_string()));
        }
    } else if !value.trim().is_empty() {
        return Err(XmlError::Invalid("text outside root".into()));
    }
    Ok(())
}

fn element(start: &BytesStart<'_>, reader: &Reader<&[u8]>, used: &mut usize, limits: &Limits) -> Result<El, XmlError> {
    let qualified = std::str::from_utf8(start.name().as_ref()).map_err(|e| XmlError::Invalid(e.to_string()))?.to_string();
    let (prefix, name) = qualified.split_once(':').map_or((None, qualified.as_str()), |(p, n)| (Some(p.to_string()), n));
    if name.is_empty() {
        return Err(XmlError::Invalid("empty element name".into()));
    }
    let mut el = El { name: name.to_string(), prefix, ..El::default() };
    for attribute in start.attributes().with_checks(limits.read_mode == ReadMode::Strict) {
        let attribute = match attribute {
            Ok(attribute) => attribute,
            Err(error) if limits.read_mode == ReadMode::Lenient => {
                let _ = error;
                continue;
            }
            Err(error) => return Err(XmlError::Invalid(error.to_string())),
        };
        if el.attrs.len() >= 1024 {
            return Err(XmlError::Limit("attributes per element"));
        }
        let key = std::str::from_utf8(attribute.key.as_ref()).map_err(|e| XmlError::Invalid(e.to_string()))?.to_string();
        let value =
            attribute.decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder()).map(|value| value.into_owned()).or_else(|error| {
                if limits.read_mode == ReadMode::Lenient {
                    Ok(String::from_utf8_lossy(&attribute.value).into_owned())
                } else {
                    Err(XmlError::Invalid(error.to_string()))
                }
            })?;
        *used = used
            .checked_add(key.len())
            .and_then(|n| n.checked_add(value.len()))
            .filter(|n| *n <= limits.max_attr_bytes)
            .ok_or(XmlError::Limit("attribute bytes"))?;
        el.attrs.push((key, value));
    }
    Ok(el)
}

fn utf8(bytes: &[u8], limit: usize, mode: ReadMode) -> Result<Cow<'_, [u8]>, XmlError> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Ok(Cow::Borrowed(rest));
    }
    let endian = if bytes.starts_with(&[0xFF, 0xFE]) || (bytes.len() >= 4 && bytes[1] == 0 && bytes[3] == 0) {
        Some(true)
    } else if bytes.starts_with(&[0xFE, 0xFF]) || (bytes.len() >= 4 && bytes[0] == 0 && bytes[2] == 0) {
        Some(false)
    } else {
        None
    };
    let Some(little) = endian else {
        return Ok(Cow::Borrowed(bytes));
    };
    let content = if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) { &bytes[2..] } else { bytes };
    if content.len() % 2 != 0 && mode == ReadMode::Strict {
        return Err(XmlError::Invalid("odd UTF-16 byte count".into()));
    }
    let units: Vec<u16> = content
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) })
        .collect();
    let decoded = if mode == ReadMode::Lenient {
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf16(&units).map_err(|error| XmlError::Invalid(error.to_string()))?
    };
    let decoded = if let Some(decl) = decoded.strip_prefix("<?xml") {
        let end = decl.find("?>").ok_or_else(|| XmlError::Invalid("unclosed XML declaration".into()))?;
        &decl[end + 2..]
    } else {
        &decoded
    };
    if decoded.len() > limit {
        return Err(XmlError::Limit("input bytes"));
    }
    Ok(Cow::Owned(decoded.as_bytes().to_vec()))
}

fn repair_ampersands(bytes: &[u8], limit: usize) -> Result<Vec<u8>, XmlError> {
    let mut repaired = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'&' {
            let rest = &bytes[index + 1..];
            let valid = rest.iter().take_while(|ch| ch.is_ascii_alphanumeric() || matches!(**ch, b'#' | b'x' | b'X')).count();
            if valid == 0 || rest.get(valid) != Some(&b';') {
                repaired.extend_from_slice(b"&amp;");
                index += 1;
                if repaired.len() > limit {
                    return Err(XmlError::Limit("input bytes"));
                }
                continue;
            }
        }
        repaired.push(bytes[index]);
        index += 1;
    }
    Ok(repaired)
}

/// Parses a whole XML part. Rejects malformed end tags, unknown entities, and over-budget input.
pub fn parse(bytes: &[u8], limits: &Limits) -> Result<El, XmlError> {
    parse_impl(bytes, limits, false)
}

/// Like [`parse`] but a `<!DOCTYPE ...>` declaration is skipped instead of rejected. Nothing in
/// the declaration is interpreted: no internal or external entity is defined, loaded or
/// expanded, so a reference to a declared entity is still an unknown-entity error (strict) or
/// dropped (lenient).
pub fn parse_skipping_doctype(bytes: &[u8], limits: &Limits) -> Result<El, XmlError> {
    parse_impl(bytes, limits, true)
}

fn parse_impl(bytes: &[u8], limits: &Limits, skip_doctype: bool) -> Result<El, XmlError> {
    if bytes.len() > limits.max_total_bytes {
        return Err(XmlError::Limit("input bytes"));
    }
    let data = utf8(bytes, limits.max_total_bytes, limits.read_mode)?;
    let data = if limits.read_mode == ReadMode::Lenient { Cow::Owned(repair_ampersands(&data, limits.max_total_bytes)?) } else { data };
    let mut reader = Reader::from_reader(data.as_ref());
    reader.config_mut().trim_text(false);
    if limits.read_mode == ReadMode::Lenient {
        reader.config_mut().check_end_names = false;
        reader.config_mut().allow_unmatched_ends = true;
    }
    let mut stack = Vec::new();
    let mut scopes: Vec<BTreeMap<String, String>> = Vec::new();
    let mut used_uris = BTreeMap::new();
    let mut root = None;
    let mut nodes = 0usize;
    let mut text_bytes = 0usize;
    let mut attr_bytes = 0usize;
    let mut buffer = Vec::new();
    loop {
        if limits.read_mode == ReadMode::Lenient && root.is_some() {
            break;
        }
        let event = reader.read_event_into(&mut buffer).map_err(|e| XmlError::Invalid(e.to_string()))?;
        let is_start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                nodes = nodes.checked_add(1).filter(|n| *n <= limits.max_nodes).ok_or(XmlError::Limit("nodes"))?;
                let depth = stack.len() + 1;
                if depth > limits.max_depth {
                    return Err(XmlError::Limit("depth"));
                }
                let mut el = element(&start, &reader, &mut attr_bytes, limits)?;
                if limits.namespaces == Namespaces::Uri {
                    let mut scope = BTreeMap::new();
                    for (key, value) in &el.attrs {
                        if key == "xmlns" {
                            scope.insert(String::new(), value.clone());
                        }
                        if let Some(prefix) = key.strip_prefix("xmlns:") {
                            scope.insert(prefix.into(), value.clone());
                        }
                    }
                    scopes.push(scope);
                    canonicalize(&mut el, &scopes, &mut used_uris);
                    if !is_start {
                        scopes.pop();
                    }
                }
                if is_start {
                    stack.push(el);
                } else {
                    attach(&mut stack, &mut root, el)?;
                }
            }
            Event::End(_) => {
                if let Some(el) = stack.pop() {
                    if limits.namespaces == Namespaces::Uri {
                        scopes.pop();
                    }
                    attach(&mut stack, &mut root, el)?;
                } else if limits.read_mode == ReadMode::Strict {
                    return Err(XmlError::Invalid("unmatched end tag".into()));
                }
            }
            Event::Text(text) if stack.is_empty() && limits.read_mode == ReadMode::Lenient => {
                let _ = text;
            }
            Event::Text(text) => {
                let raw: &[u8] = text.as_ref();
                if limits.read_mode == ReadMode::Strict && raw.contains(&b'&') {
                    return Err(XmlError::Invalid("stray ampersand".into()));
                }
                let value = text.decode().map_err(|e| XmlError::Invalid(e.to_string()))?;
                add_text(&mut stack, &value.replace("\r\n", "\n").replace('\r', "\n"), &mut text_bytes, limits.max_text_bytes)?;
            }
            Event::CData(text) => {
                let value = text.decode().map_err(|e| XmlError::Invalid(e.to_string()))?;
                add_text(&mut stack, &value.replace("\r\n", "\n").replace('\r', "\n"), &mut text_bytes, limits.max_text_bytes)?;
            }
            Event::GeneralRef(reference) => {
                let value = if reference.is_char_ref() {
                    match reference.resolve_char_ref() {
                        Ok(Some(character)) => character.to_string(),
                        _ if limits.read_mode == ReadMode::Lenient => String::new(),
                        Ok(None) => return Err(XmlError::Invalid("invalid character reference".into())),
                        Err(error) => return Err(XmlError::Invalid(error.to_string())),
                    }
                } else {
                    match reference.decode().map_err(|e| XmlError::Invalid(e.to_string()))?.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ if limits.read_mode == ReadMode::Lenient => "",
                        _ => return Err(XmlError::Invalid("unknown entity".into())),
                    }
                    .to_string()
                };
                add_text(&mut stack, &value, &mut text_bytes, limits.max_text_bytes)?;
            }
            Event::Eof => break,
            Event::DocType(_) if skip_doctype => {}
            Event::DocType(_) => return Err(XmlError::Invalid("DTD is not supported".into())),
            _ => {}
        }
        buffer.clear();
    }
    if limits.read_mode == ReadMode::Lenient {
        while let Some(el) = stack.pop() {
            attach(&mut stack, &mut root, el)?;
        }
    } else if !stack.is_empty() {
        return Err(XmlError::Invalid("unclosed element".into()));
    }
    let mut root = root.ok_or_else(|| XmlError::Invalid("missing root".into()))?;
    if limits.namespaces == Namespaces::Uri {
        for (prefix, uri) in used_uris {
            root.set_attr(&format!("xmlns:{prefix}"), &uri);
        }
    }
    Ok(root)
}
