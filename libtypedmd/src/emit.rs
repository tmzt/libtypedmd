//! The emitter (spec §8).
//!
//! **Emission always runs from the graph.** Nothing in [`crate::model`] can
//! hold document- or stream-level source text, so there is no stored input
//! for this module to return. `emit(parse(input))` is NOT required to equal
//! `input`; what is required is that the graph survives, and that the output
//! is canonical, so the emitter has a fixed point.

use crate::model::{Document, Node, NodeKind, Payload, Placement, Stream, Text, TextPart};
use crate::row::{colon_anywhere, stops_anywhere};

/// Emit a whole stream. Documents are joined by a single `---` line, blank
/// separated so the pre-pass sees it as structural (spec §6.1 rule 1).
pub fn emit(stream: &Stream) -> String {
    let mut out = String::new();
    for (i, doc) in stream.documents.iter().enumerate() {
        if i > 0 {
            out.push_str("\n---\n\n");
        }
        out.push_str(&emit_document(doc));
    }
    out
}

/// Emit one document: its envelope, then its body.
pub fn emit_document(doc: &Document) -> String {
    let mut e = Emitter::default();
    if !doc.envelope.is_empty() {
        e.line("---".to_string());
        for (key, value) in doc.envelope.entries() {
            e.line(format!("{key}: {value}"));
        }
        e.line("---".to_string());
    }
    for payload in &doc.payloads {
        e.gap();
        e.payload(payload, 0);
    }
    e.level(&doc.nodes, 0);
    e.finish()
}

#[derive(Default)]
struct Emitter {
    lines: Vec<String>,
}

impl Emitter {
    fn line(&mut self, text: String) {
        self.lines.push(text.trim_end().to_string());
    }

    /// One blank line between top-level blocks (spec §8.1).
    fn gap(&mut self) {
        if self.lines.last().map(|l| !l.is_empty()).unwrap_or(false) {
            self.lines.push(String::new());
        }
    }

    fn finish(mut self) -> String {
        while self.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            self.lines.pop();
        }
        if self.lines.is_empty() {
            String::new()
        } else {
            let mut out = self.lines.join("\n");
            out.push('\n');
            out
        }
    }

    /// Emit a sequence in a position where headings are allowed.
    ///
    /// Only a SUFFIX of the sequence can be headings: a heading closes all
    /// open list nesting, so a heading followed by a row sibling would adopt
    /// that row. Spec §8.6 states the demotion; this computes it.
    fn level(&mut self, nodes: &[Node], heading_depth: usize) {
        let split = heading_split(nodes, heading_depth);
        self.children(&nodes[..split], 0);
        for node in &nodes[split..] {
            self.heading(node, heading_depth + 1);
        }
    }

    fn heading(&mut self, node: &Node, level: usize) {
        self.gap();
        let inline = inline_count(node);
        let hashes = "#".repeat(level.min(6));
        self.line(format!("{hashes} {}", row_body(node, inline)));
        for payload in &node.payloads {
            self.gap();
            self.payload(payload, 0);
        }
        self.level(&node.nodes[inline..], level);
    }

    /// Emit a sequence in a position where only rows are allowed.
    fn children(&mut self, nodes: &[Node], depth: usize) {
        let mut i = 0;
        while i < nodes.len() {
            if is_paragraph(&nodes[i]) {
                self.gap();
                self.paragraph(&nodes[i], depth);
                i += 1;
            } else {
                if depth == 0 {
                    self.gap();
                }
                while i < nodes.len() && !is_paragraph(&nodes[i]) {
                    self.row(&nodes[i], depth);
                    i += 1;
                }
            }
        }
    }

    fn row(&mut self, node: &Node, depth: usize) {
        let indent = "  ".repeat(depth);
        let inline = inline_count(node);
        self.line(format!("{indent}- {}", row_body(node, inline)));
        for payload in &node.payloads {
            self.payload(payload, depth + 1);
        }
        self.children(&node.nodes[inline..], depth + 1);
    }

    fn paragraph(&mut self, node: &Node, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = match &node.kind {
            NodeKind::Leaf(l) => &l.text,
            _ => return,
        };
        if text.is_empty() {
            return;
        }
        for line in text.split('\n') {
            if crate::native::starts_block(line) || line.trim() == "---" {
                self.line(format!("{indent}\\{line}"));
            } else {
                self.line(format!("{indent}{line}"));
            }
        }
    }

    fn payload(&mut self, payload: &Payload, depth: usize) {
        let indent = "  ".repeat(depth);
        let ch = if payload.info.contains('`') { '~' } else { '`' };
        let longest = longest_run(&payload.body, ch);
        let fence: String = std::iter::repeat(ch).take(longest.max(2) + 1).collect();
        self.line(format!("{indent}{fence}{}", payload.info));
        for line in payload.body.lines() {
            self.lines.push(format!("{indent}{line}"));
        }
        self.line(format!("{indent}{fence}"));
    }
}

fn longest_run(text: &str, ch: char) -> usize {
    let mut best = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == ch {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

/// A leaf emits as a paragraph block when it says so — or when it has no
/// choice: a row is one line, so a value containing a newline cannot be one.
/// Demoting the PRESENTATION flag loses `form`; emitting a broken row would
/// lose the VALUE, and the value is the thing the node is for.
fn is_paragraph(node: &Node) -> bool {
    matches!(
        &node.kind,
        NodeKind::Leaf(l) if l.form == crate::model::LeafForm::Paragraph || l.text.contains('\n')
    )
}

/// An object carried as a heading, or an UNTAGGED object — which has no row
/// spelling at all (`- head` would read back as a Leaf), so a heading is the
/// only thing it can be.
fn is_heading_candidate(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Object(o) => o.carrier == crate::model::Carrier::Heading || o.tag.is_none(),
        _ => false,
    }
}

fn heading_split(nodes: &[Node], heading_depth: usize) -> usize {
    if heading_depth + 1 > 6 {
        return nodes.len();
    }
    let mut k = nodes.len();
    while k > 0 && is_heading_candidate(&nodes[k - 1]) {
        k -= 1;
    }
    k
}

// -- row bodies --------------------------------------------------------------

/// How many leading children emit as inline `name=value` on this row
/// (spec §8.5).
fn inline_count(node: &Node) -> usize {
    // A member row's tail is a colon-RHS; only an object row takes inline
    // properties (spec §3, `tail`).
    if node.as_object().is_none() {
        return 0;
    }
    node.nodes.iter().take_while(|c| inline_eligible(c)).count()
}

fn inline_eligible(node: &Node) -> bool {
    let Some(m) = node.as_member() else {
        return false;
    };
    m.placement == Placement::Inline
        && m.sigil.is_none()
        && m.sink.is_none()
        && node.nodes.is_empty()
        && node.payloads.is_empty()
        && bare_identifier(&m.name)
        && m.rhs.as_ref().map(inline_spellable).unwrap_or(false)
}

fn inline_spellable(text: &Text) -> bool {
    text.as_slot().is_some() || text.as_literal().is_some()
}

fn bare_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The row after its marker: annotation, head, tail, sink (spec §8.2).
fn row_body(node: &Node, inline: usize) -> String {
    let mut seg: Vec<String> = Vec::new();
    match &node.kind {
        NodeKind::Object(o) => {
            let tag = o.tag.as_deref().filter(|t| valid_token(t));
            if let Some(t) = tag {
                seg.push(format!("[{t}]"));
            }
            // An untagged row has no annotation to separate the head from a
            // sigil or a colon, so the head must carry those terminators.
            let untagged = tag.is_none();
            seg.push(emit_text(&o.head, true, untagged, untagged));
            for child in &node.nodes[..inline] {
                let m = child.as_member().expect("eligibility checked");
                let rhs = m.rhs.as_ref().expect("eligibility checked");
                seg.push(format!("{}={}", m.name, inline_value(rhs)));
            }
            if let Some(path) = &o.sink {
                seg.push(format!("-> {path}"));
            }
        }
        NodeKind::Member(m) => {
            if let Some(s) = m.sigil {
                seg.push(format!("[{}]", s.token()));
            }
            // Without a sigil the colon is the ONLY thing that makes this row
            // a member rather than a leaf, so it is emitted even when there
            // is no RHS (spec §3, rule 2).
            let colon = m.rhs.is_some() || m.sigil.is_none();
            let name = emit_name(&m.name, m.sigil.is_none());
            seg.push(if colon { format!("{name}:") } else { name });
            if let Some(rhs) = &m.rhs {
                seg.push(emit_text(rhs, false, false, false));
            }
            if let Some(path) = &m.sink {
                seg.push(format!("-> {path}"));
            }
        }
        NodeKind::Leaf(l) => seg.push(emit_leaf(&l.text)),
        NodeKind::Fallback(f) => seg.push(f.text.clone()),
    }
    seg.into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && !token
            .chars()
            .any(|c| c.is_whitespace() || c == '[' || c == ']')
}

fn emit_text(text: &Text, quote_first: bool, colon: bool, bracket: bool) -> String {
    let mut out = String::new();
    for (i, part) in text.parts().iter().enumerate() {
        match part {
            TextPart::Slot(s) => out.push_str(&s.render()),
            TextPart::Literal(l) => {
                if i == 0 && quote_first && literal_needs_quote(l, colon, bracket) {
                    out.push_str(&quoted(l));
                } else {
                    out.push_str(&escape_braces(l));
                }
            }
        }
    }
    out
}

fn literal_needs_quote(text: &str, colon: bool, bracket: bool) -> bool {
    text != text.trim()
        || text.starts_with('"')
        || (bracket && text.starts_with('['))
        || stops_anywhere(text, colon)
}

fn emit_name(name: &str, bracket: bool) -> String {
    if name != name.trim()
        || name.starts_with('"')
        || (bracket && name.starts_with('['))
        || stops_anywhere(name, true)
    {
        quoted(name)
    } else {
        escape_braces(name)
    }
}

/// A leaf's text is its VALUE. A row that is entirely one double-quoted
/// string reads back as the unquoted content, which is what lets a leaf hold
/// `a: b` without turning into a member on the next parse.
fn emit_leaf(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    if text != text.trim()
        || text.starts_with('"')
        || text.starts_with('[')
        || text.contains('\n')
        || colon_anywhere(text)
    {
        quoted(text)
    } else {
        text.to_string()
    }
}

fn inline_value(text: &Text) -> String {
    if let Some(slot) = text.as_slot() {
        return slot.render();
    }
    let literal = text.as_literal().unwrap_or("");
    let bare = !literal.is_empty()
        && !literal.chars().any(char::is_whitespace)
        && !literal.contains('"')
        && !literal.contains('=')
        && !literal.contains("{{");
    if bare {
        literal.to_string()
    } else {
        quoted(literal)
    }
}

/// Double-quoted with `\"` and `\\` (spec §8.4).
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// `\{{` is a literal `{{` (spec §3.2). Exactly one backslash is added, and
/// the scanner removes exactly one, so text containing backslashes survives.
fn escape_braces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < text.len() {
        if bytes[i] == b'{' && bytes.get(i + 1) == Some(&b'{') {
            out.push_str("\\{{");
            i += 2;
            continue;
        }
        let ch = text[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
