//! The object graph (spec §1).
//!
//! **There is no source-text field anywhere in this module, at any level.**
//! A [`Stream`], a [`Document`] and a [`Node`] carry structure only, so an
//! emitter has nothing to echo: emission is structurally obliged to run from
//! the graph. The ancestor of this library stored the document source on the
//! document and returned it from `to_markdown()`, which turned its round-trip
//! test into a `String::clone` assertion. The absence below is the fix.
//!
//! The only verbatim text in the graph is a *value*: a [`Leaf`]'s text, a
//! [`Fallback`]'s span, a payload body. Each is one node's own content, never
//! a cache of the document that carried it.

use crate::sigil::Sigil;

/// Where something was read from. Locations live in [`Claim`] only, and are
/// excluded from graph equality (spec §8 guarantee 1: "ignoring locations").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Location {
    /// 0-based index of the document within the stream.
    pub document: usize,
    /// 1-based line within the whole input.
    pub line: usize,
    /// 1-based byte column within the line.
    pub column: usize,
}

/// The file: an ordered sequence of documents (spec §1).
///
/// The core NEVER merges documents. Two documents that restate the same
/// object are stored as two trees, in file order (Part II §5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stream {
    pub documents: Vec<Document>,
}

impl Stream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(documents: Vec<Document>) -> Self {
        Self { documents }
    }
}

/// A `---`-delimited unit: an envelope plus a tree of nodes under an implicit
/// root. The implicit root is this struct; it has no row of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub envelope: Envelope,
    /// Payloads attached above any heading (spec §5: "a fence at heading
    /// level attaches to the innermost open heading Object" — the implicit
    /// root when none is open).
    pub payloads: Vec<Payload>,
    pub nodes: Vec<Node>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_nodes(nodes: Vec<Node>) -> Self {
        Self { nodes, ..Self::default() }
    }
}

/// The emission plane (spec §1.1): flat `key: value` lines, values opaque,
/// no nesting. Facts about *this emission* of the document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Envelope {
    entries: Vec<(String, String)>,
}

impl Envelope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    /// Insert, last-occurrence-wins (spec §5): an existing key is removed and
    /// the new pair appended, so the surviving pair keeps the LAST position.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        self.entries.retain(|(k, _)| k != &key);
        self.entries.push((key, value.into()));
    }
}

/// A fenced block attached to a node. Bodies are literal: no slot or
/// annotation recognition inside them (spec §5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Payload {
    /// Info string, verbatim. Which info string is a label and which is code
    /// is consumer semantics (Part II §6).
    pub info: String,
    pub body: String,
}

impl Payload {
    /// The body is normalised to end with a newline, which is the shape a
    /// parse produces: a fence's body is a sequence of whole lines. Building
    /// one without the newline and emitting it would otherwise come back
    /// slightly different.
    pub fn new(info: impl Into<String>, body: impl Into<String>) -> Self {
        let mut body = body.into();
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        Self { info: info.into(), body }
    }
}

/// A node in the tree.
///
/// **`kind` and `nodes` are separate fields, and that is the point.** Spec §4:
/// INDENTATION IS THE ONLY THING THAT NESTS; an annotation never changes a
/// node's depth. Rewriting `- [abc] title` to `- [Badge] title` rewrites
/// `kind` and cannot reach `nodes`, because they are different fields of the
/// same struct. The invariant is encoded in the type, not merely tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    /// Children: members, sub-objects and leaves in ONE ordered sequence
    /// (spec §4.4).
    pub nodes: Vec<Node>,
    pub payloads: Vec<Payload>,
}

impl Node {
    pub fn new(kind: NodeKind) -> Self {
        Self { kind, nodes: Vec::new(), payloads: Vec::new() }
    }

    /// An object with a consumer tag: `- [Card] Profile`.
    pub fn object(tag: impl Into<String>, head: Text) -> Self {
        Self::new(NodeKind::Object(Object {
            tag: Some(tag.into()),
            head,
            sink: None,
            carrier: Carrier::Row,
        }))
    }

    /// An untagged object — what an unannotated heading row produces.
    pub fn untagged(head: Text) -> Self {
        Self::new(NodeKind::Object(Object {
            tag: None,
            head,
            sink: None,
            carrier: Carrier::Heading,
        }))
    }

    pub fn member(name: impl Into<String>) -> Self {
        Self::new(NodeKind::Member(Member {
            name: name.into(),
            sigil: None,
            rhs: None,
            sink: None,
            placement: Placement::Row,
        }))
    }

    /// `- [abc] id`: a member typed by a core sigil.
    pub fn typed(sigil: Sigil, name: impl Into<String>) -> Self {
        let mut n = Self::member(name);
        if let NodeKind::Member(m) = &mut n.kind {
            m.sigil = Some(sigil);
        }
        n
    }

    pub fn leaf(text: impl Into<String>) -> Self {
        Self::new(NodeKind::Leaf(Leaf { text: text.into(), form: LeafForm::Row }))
    }

    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::new(NodeKind::Leaf(Leaf { text: text.into(), form: LeafForm::Paragraph }))
    }

    // -- builder helpers ---------------------------------------------------

    pub fn with_rhs(mut self, rhs: Text) -> Self {
        if let NodeKind::Member(m) = &mut self.kind {
            m.rhs = Some(rhs);
        }
        self
    }

    pub fn with_sink(mut self, path: impl Into<String>) -> Self {
        let path = path.into();
        match &mut self.kind {
            NodeKind::Object(o) => o.sink = Some(path),
            NodeKind::Member(m) => m.sink = Some(path),
            _ => {}
        }
        self
    }

    pub fn with_carrier(mut self, carrier: Carrier) -> Self {
        if let NodeKind::Object(o) = &mut self.kind {
            o.carrier = carrier;
        }
        self
    }

    pub fn with_placement(mut self, placement: Placement) -> Self {
        if let NodeKind::Member(m) = &mut self.kind {
            m.placement = placement;
        }
        self
    }

    pub fn with_child(mut self, child: Node) -> Self {
        self.nodes.push(child);
        self
    }

    pub fn with_children(mut self, children: Vec<Node>) -> Self {
        self.nodes.extend(children);
        self
    }

    pub fn with_payload(mut self, payload: Payload) -> Self {
        self.payloads.push(payload);
        self
    }

    // -- accessors ---------------------------------------------------------

    pub fn as_object(&self) -> Option<&Object> {
        match &self.kind {
            NodeKind::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_member(&self) -> Option<&Member> {
        match &self.kind {
            NodeKind::Member(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_leaf(&self) -> Option<&Leaf> {
        match &self.kind {
            NodeKind::Leaf(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_fallback(&self) -> Option<&Fallback> {
        match &self.kind {
            NodeKind::Fallback(f) => Some(f),
            _ => None,
        }
    }
}

/// Exactly four kinds (spec §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Object(Object),
    Member(Member),
    Leaf(Leaf),
    Fallback(Fallback),
}

/// A tagged or untagged object. Its inline `attr=value` pairs are NOT a
/// separate field: they are [`Member`] children with [`Placement::Inline`],
/// because `- name: value` and `name=value` produce the same node (spec §1.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Object {
    /// The bracketed token, verbatim, minus the brackets. `None` for an
    /// untagged row. The core never interprets it (Part II §2).
    pub tag: Option<String>,
    pub head: Text,
    pub sink: Option<String>,
    /// Presentation only (spec §8.6).
    pub carrier: Carrier,
}

/// One key, one right-hand side. Whether the RHS reads as a type or a value
/// is the consumer's reading, never the core's (Part II §3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Member {
    pub name: String,
    pub sigil: Option<Sigil>,
    pub rhs: Option<Text>,
    pub sink: Option<String>,
    /// Presentation only; consumers must ignore it (spec §1.2).
    pub placement: Placement,
}

/// Verbatim text. Its text is its *value*, not a parse cache (spec §1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Leaf {
    pub text: String,
    /// Presentation only: was it a list row or a paragraph.
    pub form: LeafForm,
}

/// A verbatim span plus a claim record (spec §7). Produced only by lenient
/// parsing; in strict mode a fallback whose claim shape is not
/// [`ClaimShape::None`] is the parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fallback {
    pub text: String,
    pub claim: Claim,
}

/// Which row spelling a member was seen in (spec §1.2). Presentation only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// `- name: value`, its own row. The default for programmatic nodes
    /// (spec §8.5).
    #[default]
    Row,
    /// `name=value` on the parent's row.
    Inline,
}

/// Which row spelling an object was seen in (spec §8.6). Presentation only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Carrier {
    /// A list row. The default for programmatic nodes (spec §8.6).
    #[default]
    Row,
    /// An ATX heading.
    Heading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeafForm {
    #[default]
    Row,
    Paragraph,
}

/// What an unresolved row CLAIMED to be (spec §7). The claim, not the text,
/// is what strict mode judges: unannotated prose claims nothing and is
/// preserved in both modes.
#[derive(Debug, Clone, Eq)]
pub struct Claim {
    pub shape: ClaimShape,
    /// The bracketed token, verbatim, if any.
    pub token: Option<String>,
    /// The `(name, value)` pairs as scanned, in order.
    pub attrs: Vec<(String, String)>,
    pub location: Location,
}

/// Graph equality ignores locations (spec §8 guarantee 1).
impl PartialEq for Claim {
    fn eq(&self, other: &Self) -> bool {
        self.shape == other.shape && self.token == other.token && self.attrs == other.attrs
    }
}

impl Claim {
    pub fn none(location: Location) -> Self {
        Self { shape: ClaimShape::None, token: None, attrs: Vec::new(), location }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClaimShape {
    /// The row claimed nothing: unannotated prose.
    #[default]
    None,
    /// The row led with a bracketed token.
    Sigil,
    /// The row contained `name=` pairs.
    Attrs,
}

/// Opaque text that may contain slots: an object head, a member RHS, an
/// inline property value.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Text {
    parts: Vec<TextPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextPart {
    Literal(String),
    Slot(Slot),
}

impl Text {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from parts, normalising: adjacent literals merge and empty
    /// literals drop, so two texts that render identically compare equal.
    pub fn from_parts(parts: Vec<TextPart>) -> Self {
        let mut out: Vec<TextPart> = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                TextPart::Literal(s) if s.is_empty() => {}
                TextPart::Literal(s) => match out.last_mut() {
                    Some(TextPart::Literal(prev)) => prev.push_str(&s),
                    _ => out.push(TextPart::Literal(s)),
                },
                slot => out.push(slot),
            }
        }
        Self { parts: out }
    }

    pub fn literal(text: impl Into<String>) -> Self {
        Self::from_parts(vec![TextPart::Literal(text.into())])
    }

    pub fn slot(slot: Slot) -> Self {
        Self::from_parts(vec![TextPart::Slot(slot)])
    }

    pub fn parts(&self) -> &[TextPart] {
        &self.parts
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The single literal, if this text is exactly one literal part.
    pub fn as_literal(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [TextPart::Literal(s)] => Some(s),
            [] => Some(""),
            _ => None,
        }
    }

    pub fn as_slot(&self) -> Option<&Slot> {
        match self.parts.as_slice() {
            [TextPart::Slot(s)] => Some(s),
            _ => None,
        }
    }

    /// The canonical rendering: literals as-is, slots as `{{ [abc] expr }}`.
    /// This is what a member NAME is made of, and it is idempotent.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                TextPart::Literal(s) => out.push_str(s),
                TextPart::Slot(s) => out.push_str(&s.render()),
            }
        }
        out
    }
}

/// A `{{ }}` inline expression: opaque text with an optional leading type
/// sigil (spec §1, §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Slot {
    pub sigil: Option<Sigil>,
    /// Opaque expression text. Resolution is wholly consumer-defined
    /// (Part II §4).
    pub expr: String,
}

impl Slot {
    pub fn new(expr: impl Into<String>) -> Self {
        Self { sigil: None, expr: expr.into() }
    }

    pub fn typed(sigil: Sigil, expr: impl Into<String>) -> Self {
        Self { sigil: Some(sigil), expr: expr.into() }
    }

    /// `{{ [abc] expr }}` — single interior padding spaces (spec §8.4).
    pub fn render(&self) -> String {
        match self.sigil {
            Some(s) => format!("{{{{ [{}] {} }}}}", s.token(), self.expr),
            None => format!("{{{{ {} }}}}", self.expr),
        }
    }
}
