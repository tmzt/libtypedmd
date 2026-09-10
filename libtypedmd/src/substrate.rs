//! The `MarkdownParser` trait: the block substrate (spec §6.2).
//!
//! The substrate supplies BLOCK STRUCTURE and VERBATIM INLINE SPANS. It never
//! interprets inlines — no emphasis, no links, no HTML, no GFM. Annotation
//! recognition happens after the block parse, on the row's verbatim source,
//! *before* any inline Markdown interpretation, so a sigil can never be eaten
//! as a CommonMark link (spec §6.2, execution order).
//!
//! **Interchangeability criterion:** two substrates conform iff they produce
//! identical block streams for every body.

/// A byte-exact slice of the body, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    /// 1-based line within the BODY the substrate was handed. The caller
    /// offsets it to the whole input.
    pub line: usize,
    /// 1-based byte column within that line.
    pub column: usize,
}

impl Span {
    pub fn new(text: impl Into<String>, line: usize, column: usize) -> Self {
        Self { text: text.into(), line, column }
    }
}

/// One block, as reported in document order with nesting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Level 1–6 plus the verbatim inline source of the heading text.
    Heading { level: u8, source: Span },
    List { ordered: bool, items: Vec<ListItem> },
    Paragraph { lines: Vec<Span> },
    CodeFence { info: String, body: String, line: usize },
    /// `***` / `___` only — the pre-pass owns `---` (spec §6.1).
    ThematicBreak,
}

/// A list item: its verbatim first-line source and its nested child blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub source: Span,
    pub blocks: Vec<Block>,
}

/// The substrate contract. `libtypedmd` ships a native implementation
/// ([`crate::native::NativeParser`]); an adapter crate may wrap another
/// Markdown library behind this trait, which is the only seam it gets.
pub trait MarkdownParser {
    /// Parse ONE document body. The pre-pass has already removed the
    /// envelope and split the stream, so `body` never contains a structural
    /// `---`.
    fn parse_body(&self, body: &str) -> Vec<Block>;
}
