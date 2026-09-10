//! The substrate seam (spec §6.2).
//!
//! # Why this file exists
//!
//! `parse_with_substrate` was public and no test called it, so the trait's
//! stated criterion — two substrates conform iff they produce identical block
//! streams for every body — could not fail. A criterion that cannot fail is
//! not a criterion.
//!
//! Two things fix that. Here: the corpus check is run against the reference
//! itself, and against DELIBERATELY BROKEN substrates that must be caught. In
//! `libtypedmd_pulldown`: a second real implementation is run against the same
//! corpus. Neither is enough alone — a check nothing fails proves nothing, and
//! a second implementation with no check proves nothing either.

use libtypedmd::conformance::{self, DivergenceKind};
use libtypedmd::{parse_with_substrate, Block, MarkdownParser, NativeParser, ParseOptions, Span};

/// The reference conforms to itself. This is the vacuous half; the tests below
/// are what give it meaning.
#[test]
fn the_reference_conforms_to_itself() {
    conformance::check(&NativeParser).expect("NativeParser must agree with NativeParser");
    assert!(conformance::divergences(&NativeParser).is_empty());
    assert!(conformance::CORPUS.len() >= 16, "the corpus was emptied");
}

/// A substrate that reports NOTHING is caught. An adapter that silently
/// dropped every block would otherwise pass a check that only asserted "no
/// panic".
#[test]
fn a_substrate_that_reports_nothing_is_caught() {
    struct Silent;
    impl MarkdownParser for Silent {
        fn parse_body(&self, _: &str) -> Vec<Block> {
            Vec::new()
        }
    }
    let divergence = conformance::check(&Silent).expect_err("an empty stream must be caught");
    assert_eq!(divergence.kind, DivergenceKind::Blocks);
}

/// A substrate that drops the LAST block of every body is caught. This is the
/// realistic adapter bug — an off-by-one at the end of the event stream — and
/// it is invisible to any test that only looks at the first node.
#[test]
fn a_substrate_that_drops_one_block_is_caught() {
    struct DropsLast;
    impl MarkdownParser for DropsLast {
        fn parse_body(&self, body: &str) -> Vec<Block> {
            let mut blocks = NativeParser.parse_body(body);
            blocks.pop();
            blocks
        }
    }
    let divergence = conformance::check(&DropsLast).expect_err("a dropped block must be caught");
    assert_eq!(divergence.kind, DivergenceKind::Blocks);
    assert!(conformance::divergences(&DropsLast).len() > 8, "one corpus entry is not a check");
}

/// **The span pin.** A substrate that REBUILDS a span from interpreted inlines
/// instead of slicing it from the body is caught. The rebuild simulated here
/// is the one every Markdown library performs for free — resolving an entity
/// and dropping a backslash escape — and it is the failure that would let a
/// sigil be eaten: annotation recognition runs on the row's verbatim source,
/// and a span that has been through inline interpretation is no longer that
/// source.
#[test]
fn a_substrate_that_interprets_inlines_is_caught() {
    struct Interprets;
    impl MarkdownParser for Interprets {
        fn parse_body(&self, body: &str) -> Vec<Block> {
            fn fix(span: &mut Span) {
                span.text = span.text.replace("&amp;", "&").replace("\\*", "*");
            }
            fn walk(blocks: &mut Vec<Block>) {
                for block in blocks {
                    match block {
                        Block::Heading { source, .. } => fix(source),
                        Block::Paragraph { lines } => lines.iter_mut().for_each(fix),
                        Block::List { items, .. } => {
                            for item in items {
                                fix(&mut item.source);
                                walk(&mut item.blocks);
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut blocks = NativeParser.parse_body(body);
            walk(&mut blocks);
            blocks
        }
    }
    conformance::check(&Interprets).expect_err("an interpreted inline must be caught");
}

/// A substrate that reports the right blocks in the wrong ORDER is caught.
/// Block streams are ordered and the graph is ordered; a set comparison would
/// miss this.
#[test]
fn a_substrate_that_reorders_blocks_is_caught() {
    struct Reversed;
    impl MarkdownParser for Reversed {
        fn parse_body(&self, body: &str) -> Vec<Block> {
            let mut blocks = NativeParser.parse_body(body);
            blocks.reverse();
            blocks
        }
    }
    conformance::check(&Reversed).expect_err("a reordered stream must be caught");
}

/// `parse_with_substrate` actually USES the substrate it is handed — the
/// entry point that had no caller. A substrate reporting one heading must
/// produce a graph with one object, whatever the input says.
#[test]
fn parse_with_substrate_uses_the_substrate_it_is_given() {
    struct AlwaysOneHeading;
    impl MarkdownParser for AlwaysOneHeading {
        fn parse_body(&self, _: &str) -> Vec<Block> {
            vec![Block::Heading { level: 1, source: Span::new("[Record] Fixed", 1, 3) }]
        }
    }
    let stream =
        parse_with_substrate("- [abc] ignored\n- [abc] entirely\n", &ParseOptions::lenient(), &AlwaysOneHeading)
            .expect("lenient");
    let nodes = &stream.documents[0].nodes;
    assert_eq!(nodes.len(), 1, "the graph came from the substrate, not the input");
    assert_eq!(nodes[0].as_object().unwrap().tag.as_deref(), Some("Record"));
    assert_eq!(nodes[0].as_object().unwrap().head.render(), "Fixed");
}

// -- the reference reader's own line rules -----------------------------------
//
// Both of the following were found by running a second substrate against the
// first over generated bodies. Neither is reachable from the corpus, and no
// hand-written test had covered them.

/// An UNTERMINATED fence's payload is what the source held, with no phantom
/// line at the end. A body ending in a newline splits to a trailing empty
/// element that is the terminator, not a line, and reading to end of input
/// used to sweep it into the payload.
#[test]
fn an_unterminated_fence_gains_no_phantom_line() {
    let blocks = NativeParser.parse_body("```rust\nfn x() {}\n");
    assert_eq!(
        blocks,
        vec![Block::CodeFence {
            info: "rust".to_string(),
            body: "fn x() {}\n".to_string(),
            line: 1
        }]
    );

    // An empty unterminated fence has an EMPTY payload, not a blank line.
    let blocks = NativeParser.parse_body("```\n");
    assert_eq!(
        blocks,
        vec![Block::CodeFence { info: String::new(), body: String::new(), line: 1 }]
    );

    // A blank line that IS in the source stays in the source.
    let blocks = NativeParser.parse_body("```\na\n\n");
    assert_eq!(
        blocks,
        vec![Block::CodeFence { info: String::new(), body: "a\n\n".to_string(), line: 1 }]
    );
}

/// A lazy continuation line continues the item it follows — the FIRST one as
/// much as the third. It used to take a properly indented line first, so
/// `- a\n  b\nc` continued and `- a\nb` did not.
#[test]
fn a_lazy_continuation_does_not_need_an_indented_line_first() {
    let with_indent = NativeParser.parse_body("- a\n  b\nc\n");
    let without = NativeParser.parse_body("- a\nb\n");
    let text = |blocks: &Vec<Block>| format!("{blocks:?}");

    assert!(text(&without).contains("\"b\""), "the continuation is somewhere: {:?}", without);
    match &without[..] {
        [Block::List { items, .. }] => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].source.text, "a");
            assert_eq!(
                items[0].blocks,
                vec![Block::Paragraph { lines: vec![Span::new("b", 2, 1)] }],
                "the lazy line is the item's, as it is when an indented line precedes it"
            );
        }
        other => panic!("expected one list, got {other:?}"),
    }
    assert!(text(&with_indent).contains("\"c\""));
}

/// A `>` line ENDS a list item's lazy continuation. It is the one construct
/// outside the five block shapes that always interrupts, so both substrates
/// can agree cheaply — and a substrate that let the item swallow it would
/// part company with every CommonMark reader on the second line of a quote.
#[test]
fn a_quote_line_ends_an_items_continuation() {
    let blocks = NativeParser.parse_body("- a\n> q\n> r\n");
    match &blocks[..] {
        [Block::List { items, .. }, Block::Paragraph { lines }] => {
            assert_eq!(items[0].source.text, "a");
            assert!(items[0].blocks.is_empty(), "the quote is not the item's");
            assert_eq!(lines.len(), 2, "and the quote's own lines are one run: {lines:?}");
            assert_eq!(lines[0].text, "> q");
            assert_eq!(lines[1].text, "> r");
        }
        other => panic!("expected a list then a paragraph, got {other:?}"),
    }
}
