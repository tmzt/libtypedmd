//! Literal blocks (spec §3 rule 6) read the same through both substrates.
//!
//! A literal block is sliced from the body, so what it holds cannot depend
//! on how a substrate reads Markdown inside it. These tests parse the same
//! documents through `PulldownParser` and `NativeParser` and require the same
//! graph, and the graph the rule gives.

use libtypedmd::conformance;
use libtypedmd::{parse_with_substrate, LeafForm, Node, ParseOptions, Stream};
use libtypedmd_pulldown::PulldownParser;

fn both(input: &str) -> Vec<Node> {
    let native = libtypedmd::parse(input);
    let pulldown: Stream =
        parse_with_substrate(input, &ParseOptions::lenient(), &PulldownParser).expect("lenient");
    assert_eq!(pulldown, native, "substrates disagree on {input:?}");
    let strict = parse_with_substrate(input, &ParseOptions::strict(), &PulldownParser)
        .unwrap_or_else(|e| panic!("strict rejected {input:?}: {e:?}"));
    assert_eq!(strict, native);
    native
        .documents
        .into_iter()
        .next()
        .expect("one document")
        .nodes
}

fn text(node: &Node) -> &str {
    &node.as_leaf().unwrap_or_else(|| panic!("{node:?}")).text
}

fn rhs(node: &Node) -> &str {
    node.as_member()
        .and_then(|m| m.rhs.as_ref())
        .and_then(|t| t.as_literal())
        .expect("literal RHS")
}

#[test]
fn a_block_of_everything_the_grammar_reads() {
    let n = both(
        "- |\n  a: b\n  x=y\n  - item\n  # heading\n  > quote\n  ```rust\n  fn f() {}\n  ```\n  {{ slot }}\n  one\n\n  two\n",
    );
    assert_eq!(n.len(), 1);
    assert_eq!(
        text(&n[0]),
        "a: b\nx=y\n- item\n# heading\n> quote\n```rust\nfn f() {}\n```\n{{ slot }}\none\n\ntwo"
    );
    assert_eq!(n[0].as_leaf().unwrap().form, LeafForm::Literal);
    assert!(n[0].nodes.is_empty() && n[0].payloads.is_empty());
}

#[test]
fn member_blocks_with_and_without_a_sigil() {
    let n = both("- say: |\n  a: b\n  c\n- [abc] shout: |\n  d\n");
    assert_eq!(rhs(&n[0]), "a: b\nc");
    assert_eq!(rhs(&n[1]), "d");
}

#[test]
fn empty_and_inline_forms() {
    let n = both("- |\n- say: |\n- | text\n  more\n- after\n");
    assert_eq!(n.len(), 4);
    assert_eq!(text(&n[0]), "");
    assert_eq!(rhs(&n[1]), "");
    assert_eq!(text(&n[2]), "| text");
    assert_eq!(text(&n[3]), "after");
}

#[test]
fn blocks_end_where_the_item_ends() {
    let n = both("- |\n  a\n\n\n  b\n- next\n- last\n");
    assert_eq!(text(&n[0]), "a\n\n\nb");
    assert_eq!(text(&n[1]), "next");
    assert_eq!(text(&n[2]), "last");
}

#[test]
fn nested_blocks_and_thematic_breaks_inside() {
    let n =
        both("- [Card] A\n  - [Card] B\n    - |\n      ***\n      a: b\n      ***\n    - after\n");
    let b = &n[0].nodes[0];
    assert_eq!(text(&b.nodes[0]), "***\na: b\n***");
    assert_eq!(text(&b.nodes[1]), "after");
}

#[test]
fn headings_and_ordered_rows() {
    let n = both("# [Card] A\n\n1. |\n   x: y\n\n## B\n\n- z\n");
    assert_eq!(text(&n[0].nodes[0]), "x: y");
}

#[test]
fn the_shared_corpus_still_agrees() {
    conformance::check(&PulldownParser).unwrap_or_else(|d| panic!("{d}"));
}
