//! Escaping XML writer. Provenance: deckcraft/crates/pptx/src/xml.rs and
//! wordcraft/crates/docx/src/xml.rs (deckcraft's distinct text/attribute rules).

use crate::dom::{El, Node};

fn allowed(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// Escapes element text, dropping XML 1.0 forbidden characters.
pub fn escape_text(value: &str) -> String {
    escape(value, false)
}
/// Escapes a double-quoted attribute, preserving whitespace through character references.
pub fn escape_attr(value: &str) -> String {
    escape(value, true)
}

fn escape(value: &str, attribute: bool) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' if attribute => result.push_str("&quot;"),
            '\'' if attribute => result.push_str("&apos;"),
            '\r' => result.push_str("&#13;"),
            '\n' if attribute => result.push_str("&#10;"),
            '\t' if attribute => result.push_str("&#9;"),
            ch if allowed(ch) => result.push(ch),
            _ => {}
        }
    }
    result
}

/// In-memory XML writer; `attr` applies to the most recent unclosed start tag.
#[derive(Default)]
pub struct XmlWriter {
    bytes: Vec<u8>,
    stack: Vec<String>,
    pending: bool,
}

impl XmlWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn declaration(&mut self) -> &mut Self {
        self.bytes.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        self
    }

    fn close_start(&mut self) {
        if self.pending {
            self.bytes.push(b'>');
            self.pending = false;
        }
    }

    pub fn start(&mut self, name: &str) -> &mut Self {
        self.close_start();
        self.bytes.push(b'<');
        self.bytes.extend_from_slice(name.as_bytes());
        self.stack.push(name.to_string());
        self.pending = true;
        self
    }

    pub fn attr(&mut self, name: &str, value: &str) -> &mut Self {
        if self.pending {
            self.bytes.push(b' ');
            self.bytes.extend_from_slice(name.as_bytes());
            self.bytes.extend_from_slice(b"=\"");
            self.bytes.extend_from_slice(escape_attr(value).as_bytes());
            self.bytes.push(b'"');
        }
        self
    }

    /// Marks whitespace-sensitive element text (such as a SpreadsheetML `<t>`).
    pub fn space_preserve(&mut self) -> &mut Self {
        self.attr("xml:space", "preserve")
    }

    pub fn text(&mut self, value: &str) -> &mut Self {
        self.close_start();
        self.bytes.extend_from_slice(escape_text(value).as_bytes());
        self
    }

    pub fn empty(&mut self) -> &mut Self {
        if self.pending {
            self.bytes.extend_from_slice(b"/>");
            self.pending = false;
            self.stack.pop();
        }
        self
    }

    pub fn end(&mut self) -> &mut Self {
        if self.pending {
            return self.empty();
        }
        if let Some(name) = self.stack.pop() {
            self.bytes.extend_from_slice(b"</");
            self.bytes.extend_from_slice(name.as_bytes());
            self.bytes.push(b'>');
        }
        self
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    /// Serialize a tree without namespace rewriting or recursion.
    pub fn element(&mut self, root: &El) -> &mut Self {
        enum Action<'a> {
            Element(&'a El),
            Text(&'a str),
            End,
        }
        let mut actions = vec![Action::Element(root)];
        while let Some(action) = actions.pop() {
            match action {
                Action::Element(el) => {
                    let name = el.prefix.as_ref().map_or_else(|| el.name.clone(), |prefix| format!("{prefix}:{}", el.name));
                    self.start(&name);
                    for (key, value) in &el.attrs {
                        self.attr(key, value);
                    }
                    if el.children.is_empty() {
                        self.empty();
                    } else {
                        actions.push(Action::End);
                        for child in el.children.iter().rev() {
                            actions.push(match child {
                                Node::El(child) => Action::Element(child),
                                Node::Text(text) => Action::Text(text),
                            });
                        }
                    }
                }
                Action::Text(text) => {
                    self.text(text);
                }
                Action::End => {
                    self.end();
                }
            }
        }
        self
    }
}
