//! Literal blocks (spec §3 rule 6): `- |` and `- name: |`.
//!
//! The other substrate is held to the same documents by
//! `conformance::CORPUS`, which carries the block cases below.

use libtypedmd::{parse, parse_with, LeafForm, Node, NodeKind, ParseOptions, Sigil, Stream};

fn nodes(input: &str) -> Vec<Node> {
    parse(input).documents.remove(0).nodes
}

fn leaf(node: &Node) -> (&str, LeafForm) {
    let l = node
        .as_leaf()
        .unwrap_or_else(|| panic!("not a leaf: {node:?}"));
    (&l.text, l.form)
}

fn rhs(node: &Node) -> &str {
    node.as_member()
        .and_then(|m| m.rhs.as_ref())
        .and_then(|t| t.as_literal())
        .unwrap_or_else(|| panic!("not a member with a literal RHS: {node:?}"))
}

/// Everything the row grammar and Markdown would otherwise read.
const HOSTILE: &str = "- |\n  a: b\n  x=y\n  - item\n  # heading\n  > quote\n  ```rust\n  fn f() {}\n  ```\n  {{ slot }}\n  one\n\n  two\n";
const HOSTILE_TEXT: &str =
    "a: b\nx=y\n- item\n# heading\n> quote\n```rust\nfn f() {}\n```\n{{ slot }}\none\n\ntwo";

#[test]
fn a_block_is_one_verbatim_leaf() {
    let n = nodes(HOSTILE);
    assert_eq!(n.len(), 1);
    assert_eq!(leaf(&n[0]), (HOSTILE_TEXT, LeafForm::Literal));
    assert!(n[0].nodes.is_empty(), "nothing nests in a block");
    assert!(n[0].payloads.is_empty(), "a fence inside a block is text");
}

#[test]
fn siblings_around_a_block_are_untouched() {
    let n = nodes("- before\n- |\n  a: b\n- after: 1\n");
    assert_eq!(n.len(), 3);
    assert_eq!(leaf(&n[0]).0, "before");
    assert_eq!(leaf(&n[1]), ("a: b", LeafForm::Literal));
    assert_eq!(n[2].as_member().unwrap().name, "after");
}

#[test]
fn common_indent_is_removed_and_the_rest_kept() {
    let n = nodes("- |\n    deep\n      deeper\n    deep\n- next\n");
    assert_eq!(leaf(&n[0]).0, "deep\n  deeper\ndeep");
    assert_eq!(n.len(), 2);
}

#[test]
fn trailing_blank_lines_are_dropped_and_inner_ones_kept() {
    let n = nodes("- |\n  a\n\n\n  b\n\n\n- next\n");
    assert_eq!(leaf(&n[0]).0, "a\n\n\nb");
    assert_eq!(n.len(), 2);
}

#[test]
fn a_member_block() {
    for (src, sigil) in [
        ("- say: |\n  hello: world\n  again\n", None),
        (
            "- [abc] say: |\n  hello: world\n  again\n",
            Some(Sigil::String),
        ),
    ] {
        let n = nodes(src);
        assert_eq!(n.len(), 1);
        let m = n[0].as_member().unwrap();
        assert_eq!(m.name, "say");
        assert_eq!(m.sigil, sigil);
        assert_eq!(rhs(&n[0]), "hello: world\nagain");
        assert!(n[0].nodes.is_empty());
    }
}

#[test]
fn a_member_block_takes_no_slots() {
    let n = nodes("- say: |\n  hi {{ [abc] who }}\n");
    let m = n[0].as_member().unwrap();
    assert_eq!(m.rhs.as_ref().unwrap().parts().len(), 1);
    assert_eq!(rhs(&n[0]), "hi {{ [abc] who }}");
}

#[test]
fn empty_blocks() {
    let n = nodes("- |\n- say: |\n- [abc] sigil: |\n- last\n");
    assert_eq!(n.len(), 4);
    assert_eq!(leaf(&n[0]), ("", LeafForm::Literal));
    assert_eq!(rhs(&n[1]), "");
    assert!(
        n[1].as_member().unwrap().rhs.is_some(),
        "an empty literal RHS, not no RHS"
    );
    assert_eq!(rhs(&n[2]), "");
    assert_eq!(leaf(&n[3]).0, "last");
}

#[test]
fn a_block_nested_two_levels_deep() {
    let n = nodes(
        "- [Card] A\n  - [Card] B\n    - |\n      a: b\n      - c\n    - after\n  - sibling\n",
    );
    let b = &n[0].nodes[0];
    assert_eq!(b.nodes.len(), 2);
    assert_eq!(leaf(&b.nodes[0]), ("a: b\n- c", LeafForm::Literal));
    assert_eq!(leaf(&b.nodes[1]).0, "after");
    assert_eq!(n[0].nodes.len(), 2);
    assert_eq!(leaf(&n[0].nodes[1]).0, "sibling");
}

#[test]
fn inline_text_after_the_bar_is_read_as_before() {
    let n = nodes("- | text\n  more\n- |x\n");
    assert_eq!(leaf(&n[0]), ("| text", LeafForm::Row));
    assert_eq!(
        n[0].nodes.len(),
        1,
        "the continuation is still a child paragraph"
    );
    assert_eq!(leaf(&n[1]), ("|x", LeafForm::Row));
}

#[test]
fn a_quoted_bar_is_not_a_block() {
    let n = nodes("- \"|\"\n  child\n");
    assert_eq!(leaf(&n[0]), ("|", LeafForm::Row));
    let n = nodes("- say: \"|\"\n");
    assert_eq!(rhs(&n[0]), "\"|\"");
}

#[test]
fn blocks_under_a_heading_and_ordered_rows() {
    let n = nodes("# [Card] A\n\n1. |\n   x: y\n");
    assert_eq!(leaf(&n[0].nodes[0]), ("x: y", LeafForm::Literal));
}

#[test]
fn strict_mode_accepts_every_block() {
    for src in [
        HOSTILE,
        "- |\n",
        "- say: |\n  a\n",
        "- [abc] say: |\n  [x] y\n  [abc def] z\n",
        "- |\n  [abc def] weird\n  [List] source=\n",
    ] {
        let strict: Stream = parse_with(src, &ParseOptions::strict())
            .unwrap_or_else(|e| panic!("strict rejected {src:?}: {e:?}"));
        assert_eq!(strict, parse(src));
    }
}

#[test]
fn a_leaf_block_is_not_a_fallback() {
    assert!(matches!(nodes("- |\n  [x\n")[0].kind, NodeKind::Leaf(_)));
}
