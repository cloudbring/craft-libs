//! Canonicalised XML comparison.
//!
//! Both documents are parsed into a canonical tree and compared: attribute order is
//! irrelevant, whitespace-only text between elements is dropped, text is trimmed, comments and
//! processing instructions are ignored, CDATA equals text, and names are compared by resolved
//! namespace URI plus local name, so `a:x` and `b:x` bound to one URI are equal and
//! `xmlns` declarations themselves are not compared.

use crate::CompareError;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, PartialEq, Eq)]
struct Element {
    ns: String,
    local: String,
    attrs: BTreeMap<(String, String), String>,
    children: Vec<Node>,
}

impl Element {
    fn display(&self) -> String {
        if self.ns.is_empty() { self.local.clone() } else { format!("{{{}}}{}", self.ns, self.local) }
    }
}

/// Compare two documents. `Ok(None)` means canonically equal; `Ok(Some(path))` is the path of
/// the first difference, e.g. `/root/item[2]/@id` or `/root/item[1]/text()`.
pub fn compare_xml(a: &str, b: &str) -> Result<Option<String>, CompareError> {
    let (ra, rb) = (parse(a)?, parse(b)?);
    Ok(diff_element(&ra, &rb, &format!("/{}", ra.display())))
}

/// Convenience: canonical equality.
pub fn xml_equal(a: &str, b: &str) -> Result<bool, CompareError> {
    Ok(compare_xml(a, b)?.is_none())
}

fn diff_element(a: &Element, b: &Element, path: &str) -> Option<String> {
    if a.ns != b.ns || a.local != b.local {
        return Some(path.to_string());
    }
    for key in a.attrs.keys().chain(b.attrs.keys()) {
        if a.attrs.get(key) != b.attrs.get(key) {
            let name = if key.0.is_empty() { key.1.clone() } else { format!("{{{}}}{}", key.0, key.1) };
            return Some(format!("{path}/@{name}"));
        }
    }
    let n = a.children.len().max(b.children.len());
    let (mut elem_idx, mut text_idx) = (BTreeMap::<String, usize>::new(), 0usize);
    for i in 0..n {
        match (a.children.get(i), b.children.get(i)) {
            (Some(Node::Element(x)), Some(Node::Element(y))) => {
                let c = elem_idx.entry(x.display()).or_insert(0);
                *c += 1;
                let child = format!("{path}/{}[{}]", x.display(), *c);
                if let Some(d) = diff_element(x, y, &child) {
                    return Some(d);
                }
            }
            (Some(Node::Text(x)), Some(Node::Text(y))) => {
                text_idx += 1;
                if x != y {
                    return Some(format!("{path}/text()[{text_idx}]"));
                }
            }
            (Some(Node::Element(x)), _) | (_, Some(Node::Element(x))) => {
                let c = elem_idx.get(&x.display()).copied().unwrap_or(0) + 1;
                return Some(format!("{path}/{}[{c}]", x.display()));
            }
            (Some(Node::Text(_)), _) | (_, Some(Node::Text(_))) => return Some(format!("{path}/text()[{}]", text_idx + 1)),
            (None, None) => {}
        }
    }
    None
}

fn err(msg: impl Into<String>) -> CompareError {
    CompareError::Xml(msg.into())
}

struct Raw {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    scope: Vec<(String, String)>,
}

fn parse(src: &str) -> Result<Element, CompareError> {
    let mut stack: Vec<Raw> = Vec::new();
    let mut root: Option<Element> = None;
    let mut i = 0;
    let bytes = src.as_bytes();
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let end = src[i..].find('<').map_or(src.len(), |p| i + p);
            let text = decode(&src[i..end])?;
            push_text(&mut stack, &text, root.is_some())?;
            i = end;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").ok_or_else(|| err("unterminated comment"))? + 3;
        } else if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").ok_or_else(|| err("unterminated CDATA"))?;
            push_text(&mut stack, &rest[9..end], root.is_some())?;
            i += end + 3;
        } else if rest.starts_with("<?") {
            i += rest.find("?>").ok_or_else(|| err("unterminated processing instruction"))? + 2;
        } else if rest.starts_with("<!") {
            i += skip_doctype(rest)?;
        } else if rest.starts_with("</") {
            let end = rest.find('>').ok_or_else(|| err("unterminated end tag"))?;
            let name = rest[2..end].trim();
            let raw = stack.pop().ok_or_else(|| err("unexpected end tag"))?;
            if raw.name != name {
                return Err(err(format!("mismatched end tag </{name}> for <{}>", raw.name)));
            }
            let el = finish(raw)?;
            attach(&mut stack, &mut root, el)?;
            i += end + 1;
        } else {
            let end = tag_end(rest).ok_or_else(|| err("unterminated start tag"))?;
            let self_closing = rest[..end].ends_with('/');
            let inner = &rest[1..if self_closing { end - 1 } else { end }];
            let (name, attrs) = parse_tag(inner)?;
            let mut scope = stack.last().map(|p| p.scope.clone()).unwrap_or_default();
            for (k, v) in &attrs {
                if k == "xmlns" {
                    scope.push((String::new(), v.clone()));
                } else if let Some(p) = k.strip_prefix("xmlns:") {
                    scope.push((p.to_string(), v.clone()));
                }
            }
            let raw = Raw { name, attrs, children: Vec::new(), scope };
            if self_closing {
                let el = finish(raw)?;
                attach(&mut stack, &mut root, el)?;
            } else {
                if root.is_some() && stack.is_empty() {
                    return Err(err("content after root element"));
                }
                stack.push(raw);
            }
            i += end + 1;
        }
    }
    if !stack.is_empty() {
        return Err(err("unclosed element"));
    }
    let mut root = root.ok_or_else(|| err("no root element"))?;
    root.normalise();
    Ok(root)
}

/// Index of the `>` closing a start tag, skipping quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn skip_doctype(s: &str) -> Result<usize, CompareError> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '>' if depth <= 0 => return Ok(i + 1),
            _ => {}
        }
    }
    Err(err("unterminated declaration"))
}

fn parse_tag(inner: &str) -> Result<(String, Vec<(String, String)>), CompareError> {
    let inner = inner.trim_end();
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let name = &inner[..name_end];
    if name.is_empty() {
        return Err(err("empty element name"));
    }
    let mut rest = inner[name_end..].trim_start();
    let mut attrs: Vec<(String, String)> = Vec::new();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| err("attribute without value"))?;
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let q = after.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or_else(|| err("unquoted attribute value"))?;
        let close = after[1..].find(q).ok_or_else(|| err("unterminated attribute value"))?;
        if attrs.iter().any(|(k, _)| *k == key) {
            return Err(err(format!("duplicate attribute {key}")));
        }
        attrs.push((key, decode(&after[1..1 + close])?));
        rest = after[close + 2..].trim_start();
    }
    Ok((name.to_string(), attrs))
}

fn resolve(scope: &[(String, String)], prefix: &str) -> Option<String> {
    scope.iter().rev().find(|(p, _)| p == prefix).map(|(_, u)| u.clone())
}

fn finish(raw: Raw) -> Result<Element, CompareError> {
    let (prefix, local) = raw.name.split_once(':').unwrap_or(("", raw.name.as_str()));
    let ns = match (prefix, resolve(&raw.scope, prefix)) {
        (_, Some(u)) => u,
        ("", None) => String::new(),
        (p, None) => return Err(err(format!("unbound prefix {p}"))),
    };
    let mut attrs = BTreeMap::new();
    for (k, v) in &raw.attrs {
        if k == "xmlns" || k.starts_with("xmlns:") {
            continue;
        }
        let key = match k.split_once(':') {
            Some((p, l)) => (resolve(&raw.scope, p).ok_or_else(|| err(format!("unbound prefix {p}")))?, l.to_string()),
            None => (String::new(), k.clone()),
        };
        attrs.insert(key, v.clone());
    }
    Ok(Element { ns, local: local.to_string(), attrs, children: raw.children })
}

fn attach(stack: &mut [Raw], root: &mut Option<Element>, el: Element) -> Result<(), CompareError> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::Element(el)),
        None if root.is_none() => *root = Some(el),
        None => return Err(err("multiple root elements")),
    }
    Ok(())
}

fn push_text(stack: &mut [Raw], text: &str, after_root: bool) -> Result<(), CompareError> {
    let Some(parent) = stack.last_mut() else {
        return if text.trim().is_empty() { Ok(()) } else { Err(err(if after_root { "text after root" } else { "text before root" })) };
    };
    // Merge with a preceding text node (text + CDATA + entity splits), trim at close.
    if let Some(Node::Text(prev)) = parent.children.last_mut() {
        prev.push_str(text);
    } else {
        parent.children.push(Node::Text(text.to_string()));
    }
    Ok(())
}

fn decode(s: &str) -> Result<String, CompareError> {
    if !s.contains('&') {
        return Ok(s.to_string());
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let semi = rest[p..].find(';').ok_or_else(|| err("unterminated entity"))? + p;
        let ent = &rest[p + 1..semi];
        match ent {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let code = ent
                    .strip_prefix("#x")
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32)
                    .ok_or_else(|| err(format!("unknown entity &{ent};")))?;
                out.push(code);
            }
        }
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

// Text normalisation happens after parse: trim each text node and drop empties. Done by
// post-processing so merged runs are trimmed once.
impl Element {
    fn normalise(&mut self) {
        let mut out = Vec::new();
        for child in self.children.drain(..) {
            match child {
                Node::Text(t) => {
                    let t = t.trim();
                    if !t.is_empty() {
                        out.push(Node::Text(t.to_string()));
                    }
                }
                Node::Element(mut e) => {
                    e.normalise();
                    out.push(Node::Element(e));
                }
            }
        }
        self.children = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_under_attribute_reorder() {
        let a = r#"<r><i a="1" b="2"/></r>"#;
        let b = r#"<r><i b="2"   a="1"></i></r>"#;
        assert_eq!(compare_xml(a, b).unwrap(), None);
    }

    #[test]
    fn whitespace_between_elements_ignored() {
        let a = "<?xml version=\"1.0\"?>\n<r>\n  <a>x</a>\n  <b/>\n</r>\n";
        let b = "<r><a>x</a><!-- c --><b/></r>";
        assert!(xml_equal(a, b).unwrap());
        assert!(xml_equal("<a>x</a>", "<a><![CDATA[x]]></a>").unwrap());
        assert!(xml_equal("<a>&lt;&#65;&#x42;</a>", "<a>&lt;AB</a>").unwrap());
    }

    #[test]
    fn namespace_prefixes_normalised() {
        let a = r#"<p:r xmlns:p="urn:x"><p:c p:k="1"/></p:r>"#;
        let b = r#"<q:r xmlns:q="urn:x"><q:c q:k="1"/></q:r>"#;
        assert!(xml_equal(a, b).unwrap());
        let d = r#"<r xmlns="urn:x"><c/></r>"#;
        let e = r#"<z:r xmlns:z="urn:x"><z:c/></z:r>"#;
        assert!(xml_equal(d, e).unwrap());
        // Different URI is different.
        let f = r#"<r xmlns="urn:y"><c/></r>"#;
        assert!(!xml_equal(d, f).unwrap());
    }

    #[test]
    fn first_difference_paths() {
        let p = |a, b| compare_xml(a, b).unwrap();
        assert_eq!(p("<r><i id='1'/><i id='2'/></r>", "<r><i id='1'/><i id='3'/></r>"), Some("/r/i[2]/@id".into()));
        assert_eq!(p("<r><a>x</a></r>", "<r><a>y</a></r>"), Some("/r/a[1]/text()[1]".into()));
        assert_eq!(p("<r><a/><b/></r>", "<r><a/></r>"), Some("/r/b[1]".into()));
        assert_eq!(p("<r/>", "<s/>"), Some("/r".into()));
        assert_eq!(p("<r><a x='1'/></r>", "<r><a/></r>"), Some("/r/a[1]/@x".into()));
    }

    #[test]
    fn malformed_is_error() {
        assert!(compare_xml("<a>", "<a/>").is_err());
        assert!(compare_xml("<a></b>", "<a/>").is_err());
        assert!(compare_xml("<p:a/>", "<a/>").is_err());
        assert!(compare_xml("", "<a/>").is_err());
    }
}
