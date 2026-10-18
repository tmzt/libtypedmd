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
        Self {
            text: text.into(),
            line,
            column,
        }
    }
}

/// One block, as reported in document order with nesting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Level 1–6 plus the verbatim inline source of the heading text.
    Heading {
        level: u8,
        source: Span,
    },
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Paragraph {
        lines: Vec<Span>,
    },
    CodeFence {
        info: String,
        body: String,
        line: usize,
    },
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

// -- shared row lexis --------------------------------------------------------
//
// Two things every substrate must agree on BYTE FOR BYTE, because they decide
// where a verbatim span starts and which lines carry no node at all. They live
// here, next to the trait, so that a second substrate cannot drift from the
// first by re-deriving them slightly differently.
//
// Everything ABOVE this line is what a substrate must report; everything in
// these two functions is the row lexis it reports it in.

/// The list marker on a line, if any: `(ordered, byte offset of the content
/// after the marker)`. `-`, `*`, `+`, and `1.` / `1)` up to nine digits.
///
/// The content offset is where a list row's verbatim span BEGINS, so a
/// substrate that computed it differently would hand annotation recognition a
/// different row (spec §6.2).
pub fn list_marker(text: &str) -> Option<(bool, usize)> {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 && text.len() == indent {
        return None;
    }
    let rest = &text[indent..];
    let bytes = rest.as_bytes();
    let (ordered, marker_len) = if matches!(bytes.first(), Some(b'-' | b'*' | b'+')) {
        (false, 1)
    } else {
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        match bytes.get(digits) {
            Some(b'.' | b')') => (true, digits + 1),
            _ => return None,
        }
    };
    match bytes.get(marker_len) {
        None => Some((ordered, indent + marker_len + 1)),
        Some(b' ') => {
            let after = &rest[marker_len..];
            let spaces = after.len() - after.trim_start_matches(' ').len();
            // A run of 5+ spaces is not part of the marker (CommonMark).
            let spaces = if spaces > 4 { 1 } else { spaces };
            Some((ordered, indent + marker_len + spaces))
        }
        _ => None,
    }
}

/// Is this line a thematic break? `***` / `___` only per spec §6.2 — but a
/// `---` that survived the pre-pass is one too, and like the others it carries
/// no node.
pub fn is_thematic_break(text: &str) -> bool {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let trimmed: String = text.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.as_bytes()[0];
    matches!(first, b'*' | b'_' | b'-') && trimmed.bytes().all(|b| b == first)
}

/// Merge adjacent lists of the same orderedness into one.
///
/// [`Block::List`] carries `ordered` and `items` and NOTHING ELSE — no bullet
/// character, no delimiter, no start ordinal (spec §3 rule 3 drops it) — so
/// "where one list ended and the next began" is not expressible in the
/// vocabulary a substrate reports in, and the assembler cannot see it either:
/// the items of two adjacent lists become siblings in exactly the sequence the
/// items of one list would. Readers disagree about it constantly — CommonMark
/// starts a new list when the bullet changes (`- a` then `* b`) and continues
/// one when the indent changes (`  - a` then `- b`); a line reader does the
/// opposite of both — so every substrate ends here, and the difference is
/// quotiented away rather than argued about.
pub fn merge_adjacent_lists(blocks: Vec<Block>) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let block = match block {
            Block::List { ordered, items } => Block::List {
                ordered,
                items: items
                    .into_iter()
                    .map(|item| ListItem {
                        source: item.source,
                        blocks: merge_adjacent_lists(item.blocks),
                    })
                    .collect(),
            },
            other => other,
        };
        match (out.last_mut(), block) {
            (
                Some(Block::List {
                    ordered: prev,
                    items: into,
                }),
                Block::List { ordered, mut items },
            ) if *prev == ordered => {
                into.append(&mut items);
            }
            (_, block) => out.push(block),
        }
    }
    out
}
