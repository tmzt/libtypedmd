//! Blocks to graph: heading nesting, list nesting, payload attachment and
//! duplicate keys (spec §4, §5).
//!
//! **INDENTATION IS THE ONLY THING THAT NESTS.** Depth here comes from the
//! substrate's block nesting and from heading levels, and from nothing else.
//! No branch in this module consults a node's annotation to decide where it
//! goes; [`crate::row::scan_row`] decides only what a node IS.

use crate::error::ParseError;
use crate::model::{
    Carrier, ClaimShape, Document, Envelope, Leaf, LeafForm, Location, Node, NodeKind, Payload,
};
use crate::prepass::{envelope_pair, RawDocument};
use crate::row::scan_row;
use crate::sigil::AliasTable;
use crate::substrate::{Block, ListItem, MarkdownParser, Span};

pub(crate) struct Assembler<'a> {
    aliases: &'a AliasTable,
    doc_index: usize,
    base_line: usize,
    pub(crate) errors: Vec<ParseError>,
}

/// A node plus where its row was, so a duplicate can be reported at the
/// second occurrence (spec §5) without putting a location field in the graph.
struct Built {
    node: Node,
    location: Location,
}

impl<'a> Assembler<'a> {
    pub(crate) fn new(aliases: &'a AliasTable, doc_index: usize) -> Self {
        Self { aliases, doc_index, base_line: 1, errors: Vec::new() }
    }

    pub(crate) fn document(
        &mut self,
        raw: &RawDocument,
        substrate: &dyn MarkdownParser,
    ) -> Document {
        let envelope = self.envelope(raw);
        self.base_line = raw.body_line;
        let blocks = substrate.parse_body(&raw.body);
        let (built, payloads) = self.blocks(&blocks);
        let at = Location { document: self.doc_index, line: raw.body_line, column: 1 };
        let nodes = self.seal(Vec::new(), built, at);
        Document { envelope, payloads, nodes }
    }

    fn envelope(&mut self, raw: &RawDocument) -> Envelope {
        let mut envelope = Envelope::new();
        for (line, text) in &raw.envelope_lines {
            if text.trim().is_empty() {
                continue;
            }
            let at = Location { document: self.doc_index, line: *line, column: 1 };
            match envelope_pair(text) {
                Some((key, value)) => {
                    if envelope.get(&key).is_some() {
                        self.errors
                            .push(ParseError::new(format!("duplicate envelope key `{key}`"), at));
                    }
                    envelope.set(key, value);
                }
                None => self.errors.push(ParseError::new(
                    "envelope line is not `key: value`; the envelope plane is flat pairs only",
                    at,
                )),
            }
        }
        envelope
    }

    fn location(&self, span: &Span) -> Location {
        Location {
            document: self.doc_index,
            line: self.base_line + span.line - 1,
            column: span.column,
        }
    }

    /// Assemble one block sequence. Returns the nodes it produced and the
    /// payloads that attached to the CONTAINER — fences seen before any
    /// heading opened (spec §5).
    fn blocks(&mut self, blocks: &[Block]) -> (Vec<Built>, Vec<Payload>) {
        let mut roots: Vec<Built> = Vec::new();
        let mut root_payloads: Vec<Payload> = Vec::new();
        // Open headings, innermost last, each with the children gathered so
        // far. A heading's children are sealed when it closes.
        let mut open: Vec<(u8, Built, Vec<Built>)> = Vec::new();

        for block in blocks {
            match block {
                Block::ThematicBreak => {}
                Block::Heading { level, source } => {
                    let at = self.location(source);
                    let mut built = self.row(&source.text, true, at);
                    if let NodeKind::Object(o) = &mut built.node.kind {
                        o.carrier = Carrier::Heading;
                    }
                    // A heading closes all open headings of equal or higher
                    // level; its parent is the nearest open one of LOWER
                    // level (the root if none). Skipped levels create no
                    // phantom nodes.
                    while open.last().map(|(l, _, _)| *l >= *level).unwrap_or(false) {
                        self.close_heading(&mut open, &mut roots);
                    }
                    open.push((*level, built, Vec::new()));
                }
                Block::List { items, .. } => {
                    let built: Vec<Built> = items.iter().map(|i| self.item(i)).collect();
                    self.place_many(&mut open, &mut roots, built);
                }
                Block::Paragraph { lines } => {
                    let text = lines
                        .iter()
                        .map(|s| unescape_line(&s.text))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let at = lines.first().map(|s| self.location(s)).unwrap_or_default();
                    let node = Node::new(NodeKind::Leaf(Leaf { text, form: LeafForm::Paragraph }));
                    self.place_many(&mut open, &mut roots, vec![Built { node, location: at }]);
                }
                Block::CodeFence { info, body, .. } => {
                    let payload = Payload::new(info.clone(), body.clone());
                    match open.last_mut() {
                        Some((_, heading, _)) => heading.node.payloads.push(payload),
                        None => root_payloads.push(payload),
                    }
                }
            }
        }

        while !open.is_empty() {
            self.close_heading(&mut open, &mut roots);
        }

        (roots, root_payloads)
    }

    fn place_many(
        &self,
        open: &mut [(u8, Built, Vec<Built>)],
        roots: &mut Vec<Built>,
        built: Vec<Built>,
    ) {
        match open.last_mut() {
            Some((_, _, children)) => children.extend(built),
            None => roots.extend(built),
        }
    }

    fn close_heading(&mut self, open: &mut Vec<(u8, Built, Vec<Built>)>, roots: &mut Vec<Built>) {
        let (_, mut heading, children) = open.pop().expect("caller checked");
        let inline = std::mem::take(&mut heading.node.nodes);
        let at = heading.location;
        heading.node.nodes = self.seal(inline, children, at);
        match open.last_mut() {
            Some((_, _, siblings)) => siblings.push(heading),
            None => roots.push(heading),
        }
    }

    fn item(&mut self, item: &ListItem) -> Built {
        let at = self.location(&item.source);
        let mut built = self.row(&item.source.text, false, at);
        let (children, payloads) = self.blocks(&item.blocks);
        built.node.payloads.extend(payloads);
        let inline = std::mem::take(&mut built.node.nodes);
        built.node.nodes = self.seal(inline, children, at);
        built
    }

    fn row(&mut self, text: &str, is_heading: bool, at: Location) -> Built {
        let scanned = scan_row(text, is_heading, self.aliases, at);
        self.errors.extend(scanned.issues);
        if let NodeKind::Fallback(f) = &scanned.kind {
            // The spec's strict predicate verbatim (§7). The `None` arm is
            // currently unreachable: a row that claims nothing becomes a Leaf
            // (spec §1, §3 rule 2), never a Fallback. The check is kept as
            // written so a future rule that DOES produce an unclaiming
            // fallback stays strict-clean by construction.
            if f.claim.shape != ClaimShape::None {
                self.errors.push(
                    ParseError::new(
                        "row claimed an annotation that could not be resolved",
                        f.claim.location,
                    )
                    .with_claim(f.claim.clone()),
                );
            }
        }
        let mut node = Node::new(scanned.kind);
        node.nodes = scanned.inline;
        Built { node, location: at }
    }

    /// Combine a row's inline members with its indented children into ONE
    /// ordered sequence (spec §4.4), resolve duplicates and drop locations.
    ///
    /// Inline and row members share one name plane, because the two spellings
    /// produce the same node (spec §1.2): `- [X] a=1` followed by `- a: 2` is
    /// a duplicate, not two members.
    fn seal(&mut self, inline: Vec<Node>, children: Vec<Built>, at: Location) -> Vec<Node> {
        let mut all: Vec<Built> = inline
            .into_iter()
            .map(|node| Built { node, location: at })
            .collect();
        all.extend(children);

        let names: Vec<Option<String>> =
            all.iter().map(|b| b.node.as_member().map(|m| m.name.clone())).collect();
        let mut keep = vec![true; all.len()];
        for i in 0..names.len() {
            let Some(name) = &names[i] else { continue };
            if let Some(j) = (i + 1..names.len()).find(|j| names[*j].as_ref() == Some(name)) {
                keep[i] = false;
                self.errors.push(ParseError::new(
                    format!("duplicate member name `{name}`"),
                    all[j].location,
                ));
            }
        }

        all.into_iter()
            .zip(keep)
            .filter(|(_, k)| *k)
            .map(|(b, _)| b.node)
            .collect()
    }
}

/// Undo the emitter's line escape: a `\` before something that would
/// otherwise open a block. Paragraph text is a VALUE, so it must survive a
/// round trip even when it looks like Markdown.
fn unescape_line(line: &str) -> String {
    match line.strip_prefix('\\') {
        Some(rest) if crate::native::starts_block(rest) || rest.trim() == "---" => rest.to_string(),
        _ => line.to_string(),
    }
}
