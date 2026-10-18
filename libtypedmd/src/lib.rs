//! `libtypedmd` — a generic object-graph format whose serialization is
//! Markdown.
//!
//! The mental model: `- property: value` is YAML encoded as Markdown. A
//! Markdown list becomes a mapping; indentation is nesting, exactly as in
//! YAML. One key, one right-hand side.
//!
//! ```
//! use libtypedmd::{parse, emit};
//!
//! let stream = parse("# [Record] User\n\n- [abc] id\n- [#] age\n- active: Boolean\n");
//! let user = &stream.documents[0].nodes[0];
//! assert_eq!(user.as_object().unwrap().tag.as_deref(), Some("Record"));
//! assert_eq!(user.nodes.len(), 3);
//!
//! // Emission runs from the graph, never from stored source.
//! assert_eq!(parse(&emit(&stream)), stream);
//! ```
//!
//! # Three things this crate will not do
//!
//! * **It has no dependencies.** Not serde, not a Markdown library. The block
//!   substrate is behind [`MarkdownParser`] so another one can be supplied;
//!   it never becomes a dependency of the core.
//! * **It stores no source text above the node.** See [`model`].
//! * **It knows nothing about any consumer.** Tags, sink paths, slot
//!   expressions and the reading of a member's RHS are all carried verbatim
//!   and interpreted above (Part II of the specification).

#![forbid(unsafe_code)]

mod assemble;
pub mod conformance;
pub mod emit;
pub mod error;
pub mod model;
pub mod native;
pub mod prepass;
pub mod row;
pub mod sigil;
pub mod substrate;

pub use emit::{emit, emit_document};
pub use error::ParseError;
pub use model::{
    Carrier, Claim, ClaimShape, Document, Envelope, Fallback, Leaf, LeafForm, Location, Member,
    Node, NodeKind, Object, Payload, Placement, Slot, Stream, Text, TextPart,
};
pub use native::NativeParser;
pub use sigil::{AliasTable, Annotation, Sigil, SIGILS};
pub use substrate::{Block, ListItem, MarkdownParser, Span};

use assemble::Assembler;

/// Parser options.
///
/// **Leniency is the default.** The producer is an LLM and will not always be
/// compliant, which is also why the annotation grammar is kept small.
#[derive(Debug, Default)]
pub struct ParseOptions {
    /// Reject invalid annotations anywhere. The predicate: the parse fails
    /// iff a node fell back with `shape != None` — it claimed to describe an
    /// object and could not be resolved. Unannotated prose claims nothing and
    /// is preserved in both modes, so a well-formed document parses
    /// identically under both.
    pub strict: bool,
    /// Consumer spellings for sigils and tags. Without one, only the seven
    /// canonical sigils match.
    pub aliases: AliasTable,
}

impl ParseOptions {
    pub fn lenient() -> Self {
        Self::default()
    }

    pub fn strict() -> Self {
        Self {
            strict: true,
            ..Self::default()
        }
    }

    pub fn with_aliases(mut self, aliases: AliasTable) -> Self {
        self.aliases = aliases;
        self
    }
}

/// Parse leniently with the native substrate. Cannot fail: anything
/// unrecognized becomes a [`Fallback`] carrying its verbatim span and a claim
/// record (spec §7).
pub fn parse(input: &str) -> Stream {
    parse_with(input, &ParseOptions::lenient()).expect("a lenient parse cannot fail")
}

/// Parse with options and the native substrate.
pub fn parse_with(input: &str, options: &ParseOptions) -> Result<Stream, ParseError> {
    parse_with_substrate(input, options, &NativeParser)
}

/// Parse with options and a caller-supplied block substrate.
pub fn parse_with_substrate(
    input: &str,
    options: &ParseOptions,
    substrate: &dyn MarkdownParser,
) -> Result<Stream, ParseError> {
    let raw = prepass::split_stream(input);
    let mut documents = Vec::with_capacity(raw.len());
    let mut errors: Vec<ParseError> = Vec::new();
    for (index, doc) in raw.iter().enumerate() {
        let mut assembler = Assembler::new(&options.aliases, index);
        documents.push(assembler.document(doc, substrate));
        errors.append(&mut assembler.errors);
    }
    if options.strict {
        errors.sort_by_key(|e| (e.location.document, e.location.line, e.location.column));
        if let Some(first) = errors.into_iter().next() {
            return Err(first);
        }
    }
    Ok(Stream::with(documents))
}
