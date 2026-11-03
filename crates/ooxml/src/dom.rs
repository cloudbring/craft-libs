//! Bounded local-name XML tree. Provenance: deckcraft/crates/pptx/src/xml.rs,
//! wordcraft/crates/docx/src/xml.rs and gridcraft/crates/xlsx/src/xml.rs.

use std::borrow::Cow;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// Per-document caps. Text and attribute budgets count decoded UTF-8 bytes across the tree.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_text_bytes: usize,
    pub max_attr_bytes: usize,
    pub max_total_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_nodes: 1_000_000,
            max_text_bytes: 96 * 1024 * 1024,
            max_attr_bytes: 1024 * 1024,
            max_total_bytes: 96 * 1024 * 1024,
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
    pub fn child(&self, name: &str) -> Option<&El> {
        self.children.iter().find_map(|node| match node {
            Node::El(el) if el.name == local(name) => Some(el),
            _ => None,
        })
    }

    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.children.iter().filter_map(move |node| match node {
            Node::El(el) if el.name == local(name) => Some(el),
            _ => None,
        })
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

    /// Concatenates direct text nodes; descendants are not included.
    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|node| match node {
                Node::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
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
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.map_err(|e| XmlError::Invalid(e.to_string()))?;
        if el.attrs.len() >= 1024 {
            return Err(XmlError::Limit("attributes per element"));
        }
        let key = std::str::from_utf8(attribute.key.as_ref()).map_err(|e| XmlError::Invalid(e.to_string()))?.to_string();
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|e| XmlError::Invalid(e.to_string()))?
            .into_owned();
        *used = used
            .checked_add(key.len())
            .and_then(|n| n.checked_add(value.len()))
            .filter(|n| *n <= limits.max_attr_bytes)
            .ok_or(XmlError::Limit("attribute bytes"))?;
        el.attrs.push((key, value));
    }
    Ok(el)
}

fn utf8(bytes: &[u8], limit: usize) -> Result<Cow<'_, [u8]>, XmlError> {
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
    if content.len() % 2 != 0 {
        return Err(XmlError::Invalid("odd UTF-16 byte count".into()));
    }
    let units: Vec<u16> = content
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) })
        .collect();
    let decoded = String::from_utf16(&units).map_err(|e| XmlError::Invalid(e.to_string()))?;
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

/// Parses a whole XML part. Rejects malformed end tags, unknown entities, and over-budget input.
pub fn parse(bytes: &[u8], limits: &Limits) -> Result<El, XmlError> {
    if bytes.len() > limits.max_total_bytes {
        return Err(XmlError::Limit("input bytes"));
    }
    let data = utf8(bytes, limits.max_total_bytes)?;
    let mut reader = Reader::from_reader(data.as_ref());
    reader.config_mut().trim_text(false);
    let mut stack = Vec::new();
    let mut root = None;
    let mut nodes = 0usize;
    let mut text_bytes = 0usize;
    let mut attr_bytes = 0usize;
    let mut buffer = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buffer).map_err(|e| XmlError::Invalid(e.to_string()))?;
        let is_start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                nodes = nodes.checked_add(1).filter(|n| *n <= limits.max_nodes).ok_or(XmlError::Limit("nodes"))?;
                let depth = stack.len() + 1;
                if depth > limits.max_depth {
                    return Err(XmlError::Limit("depth"));
                }
                let el = element(&start, &reader, &mut attr_bytes, limits)?;
                if is_start {
                    stack.push(el);
                } else {
                    attach(&mut stack, &mut root, el)?;
                }
            }
            Event::End(_) => {
                let el = stack.pop().ok_or_else(|| XmlError::Invalid("unmatched end tag".into()))?;
                attach(&mut stack, &mut root, el)?;
            }
            Event::Text(text) => {
                let value = text.decode().map_err(|e| XmlError::Invalid(e.to_string()))?;
                add_text(&mut stack, &value.replace("\r\n", "\n").replace('\r', "\n"), &mut text_bytes, limits.max_text_bytes)?;
            }
            Event::CData(text) => {
                let value = text.decode().map_err(|e| XmlError::Invalid(e.to_string()))?;
                add_text(&mut stack, &value.replace("\r\n", "\n").replace('\r', "\n"), &mut text_bytes, limits.max_text_bytes)?;
            }
            Event::GeneralRef(reference) => {
                let value = if reference.is_char_ref() {
                    reference
                        .resolve_char_ref()
                        .map_err(|e| XmlError::Invalid(e.to_string()))?
                        .ok_or_else(|| XmlError::Invalid("invalid character reference".into()))?
                        .to_string()
                } else {
                    match reference.decode().map_err(|e| XmlError::Invalid(e.to_string()))?.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return Err(XmlError::Invalid("unknown entity".into())),
                    }
                    .to_string()
                };
                add_text(&mut stack, &value, &mut text_bytes, limits.max_text_bytes)?;
            }
            Event::Eof => break,
            Event::DocType(_) => return Err(XmlError::Invalid("DTD is not supported".into())),
            _ => {}
        }
        buffer.clear();
    }
    if !stack.is_empty() {
        return Err(XmlError::Invalid("unclosed element".into()));
    }
    root.ok_or_else(|| XmlError::Invalid("missing root".into()))
}
