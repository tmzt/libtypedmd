//! The conformance corpus and the substrate comparison (spec §6.2).
//!
//! **Interchangeability criterion:** two substrates conform iff they produce
//! identical block streams for every body. That sentence is only worth
//! something if it can FAIL, and it cannot fail while there is one
//! implementation of [`MarkdownParser`]. So the corpus and the comparison
//! ship HERE, beside the trait, and any substrate — in this workspace or
//! another tree — can be run against them:
//!
//! ```
//! use libtypedmd::{conformance, NativeParser};
//! conformance::check(&NativeParser).expect("the reference conforms to itself");
//! ```
//!
//! The corpus is also what [`crate::emit`] is round-tripped over, so the two
//! guarantees are measured against ONE set of documents rather than two that
//! can drift apart.
//!
//! Tag spellings below are deliberately neutral nouns: the crate's agnosticism
//! gate scans this file like any other, and a corpus is exactly where a
//! consumer's vocabulary would otherwise slip in.

use crate::model::Stream;
use crate::native::NativeParser;
use crate::prepass::split_stream;
use crate::substrate::{Block, MarkdownParser};
use crate::{parse_with_substrate, ParseOptions};

/// Documents that between them exercise every field in spec §1.
pub const CORPUS: &[&str] = &[
    // Schema-shaped (spec §9).
    "# [Record] User\n\n- [abc] id\n- [abc] name\n- [#] age\n- [-] team -> teams.id\n- active: Boolean\n",
    // Composition-shaped, slots in a property value and in heads (spec §9).
    "# [Panel] Inbox\n\n- [List] source={{ [#] inbox.threads }} item_alias=\"thread\"\n  - [Text] {{ [abc] thread.subject }}\n  - [Text] {{ [abc] thread.sender }}\n",
    // Mixed kinds at one depth (spec §9).
    "- [Card] Profile\n  - [abc] title\n  - [Badge] Status\n  - visible: true\n",
    // Every sigil.
    "- [abc] s\n- [#] n\n- [1.] l\n- [-] r\n- [31] d\n- [_] b\n- [$] c\n",
    // Nested property sets.
    "- [Form] Signup\n  - fields:\n    - [abc] email\n    - [abc] password\n  - submit: post\n",
    // Headings, prose and a payload.
    "# Notes\n\nSome prose here.\n\n```rust\nfn main() {}\n```\n",
    // A payload inside a list item, before the item's children.
    "- [Module] m\n  ```rust\n  fn x() {}\n  ```\n  - [abc] name\n",
    // Envelope plus body.
    "---\nbase: abc123\nfrom: somewhere\n---\n\n# [Record] A\n\n- [abc] id\n",
    // Two documents restating the same object.
    "# [Record] A\n\n- [abc] id\n\n---\n\n# [Record] A\n\n- [abc] name\n",
    // Colons that are not member separators, and a quoted head.
    "- link: http://example.com/x\n- [Card] \"Notes: draft\" mode=wide\n",
    // Values that contain the grammar's own terminators, in every position
    // where the emitter has to quote to keep them from biting.
    "# \"Notes: draft\"\n\n- [Card] \"a -> b\"\n- [Card] \"a x=y\"\n- \"a: b\": v\n- [abc] \"x -> y\"\n- [List] label=\"two words\"\n",
    // Leaves, including one whose value contains a colon.
    "- \"Note: this matters\"\n- plain prose row\n",
    // Escaped braces (including a CLOSED pair, which is the case that needs
    // the escape) and an unclosed slot (lenient: literal text).
    "- head with \\{{ braces: yes\n- [Text] literal \\{{ here\n- [Text] \\{{ x }} still literal\n",
    // Fallbacks: an unresolvable annotation and an unparseable attribute.
    "- [abc def] weird\n- [List] source=\n",
    // Sinks in all three positions.
    "- name: value -> some.path\n- [-] team -> teams.id\n- [Card] X -> a.b\n",
    // Deep heading nesting.
    "# A\n\n## B\n\n### C\n\n- x: 1\n",
    // Inline Markdown that a substrate must NOT interpret: emphasis, code
    // spans, an inline link, an entity, a backslash escape — and a link
    // REFERENCE DEFINITION that makes `[Card]` a shortcut link everywhere
    // below it. Annotation recognition runs on the row's verbatim source, so
    // the tag survives and the definition line is ordinary text (spec §6.2).
    "[Card]: /somewhere\n\n- [Card] a *b* `c` [d](e) &amp; \\*f\\*\n",
    // Table pipes and a `[x]` row: both are GFM, and GFM is OFF, so both are
    // ordinary text and `[_]` keeps the boolean sigil to itself (spec §2).
    "| a | b |\n| --- | --- |\n\n- [_] done\n- [x] not a sigil\n",
    // Literal blocks: everything the row grammar and Markdown would otherwise
    // read, taken verbatim (spec §3 rule 6); a block between ordinary rows.
    "- before\n- |\n  a: b\n  x=y\n  - item\n  # heading\n  > quote\n  ```rust\n  fn f() {}\n  ```\n  {{ slot }}\n  one\n\n  two\n- after: 1\n",
    // Member blocks, with and without a sigil, empty blocks, `- | text`
    // read as a row, and a quoted bar.
    "- say: |\n  hello: world\n  again\n- [abc] shout: |\n  LOUD -> x\n- empty: |\n- |\n- | text\n- \"|\"\n",
    // A block nested two list levels deep, with a heading above.
    "# [Card] A\n\n- [Card] B\n  - |\n    a: b\n    - c\n    ***\n\n    d\n  - after\n",
];

/// Where two substrates disagreed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// Index within [`CORPUS`], or whatever index the caller passed.
    pub document: usize,
    /// Index of the body within that document's stream.
    pub body: usize,
    pub kind: DivergenceKind,
    pub source: String,
    /// The reference rendering (`{:#?}` of blocks, or of the graph).
    pub reference: String,
    /// The candidate's rendering of the same thing.
    pub candidate: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceKind {
    /// The block streams differ — the criterion in spec §6.2.
    Blocks,
    /// The block streams differ in a way that survives into the graph. Every
    /// `Graph` divergence is also a `Blocks` one; the reverse does not hold,
    /// because the assembler ignores distinctions the trait does not carry
    /// (two adjacent lists, say, place their items in the same parent).
    Graph,
}

impl std::fmt::Display for Divergence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} divergence on document {} body {}:\n--- source ---\n{}\n--- reference ---\n{}\n--- candidate ---\n{}",
            self.kind, self.document, self.body, self.source, self.reference, self.candidate
        )
    }
}

/// Run `candidate` against [`CORPUS`] and return the FIRST divergence from
/// [`NativeParser`], if any.
pub fn check(candidate: &dyn MarkdownParser) -> Result<(), Divergence> {
    match divergences(candidate).into_iter().next() {
        Some(first) => Err(first),
        None => Ok(()),
    }
}

/// Every divergence between `candidate` and [`NativeParser`] over [`CORPUS`].
pub fn divergences(candidate: &dyn MarkdownParser) -> Vec<Divergence> {
    let mut out = Vec::new();
    for (index, document) in CORPUS.iter().enumerate() {
        out.extend(compare(index, document, candidate));
    }
    out
}

/// Compare one whole document — every body the pre-pass carves out of it,
/// then the graph the two substrates assemble from it.
pub fn compare(index: usize, document: &str, candidate: &dyn MarkdownParser) -> Vec<Divergence> {
    let mut out = Vec::new();
    for (body_index, raw) in split_stream(document).iter().enumerate() {
        let reference = NativeParser.parse_body(&raw.body);
        let theirs = candidate.parse_body(&raw.body);
        if reference != theirs {
            out.push(Divergence {
                document: index,
                body: body_index,
                kind: DivergenceKind::Blocks,
                source: raw.body.clone(),
                reference: render_blocks(&reference),
                candidate: render_blocks(&theirs),
            });
        }
    }

    let reference = graph(document, &NativeParser);
    let theirs = graph(document, candidate);
    if reference != theirs {
        out.push(Divergence {
            document: index,
            body: 0,
            kind: DivergenceKind::Graph,
            source: document.to_string(),
            reference: format!("{reference:#?}"),
            candidate: format!("{theirs:#?}"),
        });
    }
    out
}

fn graph(document: &str, substrate: &dyn MarkdownParser) -> Stream {
    parse_with_substrate(document, &ParseOptions::lenient(), substrate)
        .expect("a lenient parse cannot fail")
}

fn render_blocks(blocks: &[Block]) -> String {
    format!("{blocks:#?}")
}
