//! **INDENTATION IS THE ONLY THING THAT NESTS** (spec §4).
//!
//! This is the format's central invariant and the reason the data model puts
//! `kind` and `nodes` in separate fields. The tests below assert it from both
//! directions: parsing rows that differ only in their annotation must produce
//! the same shape, and rewriting a node's kind in the graph must leave its
//! children untouched.

use libtypedmd::{parse, Node, NodeKind, Sigil};

/// Spec §9, third example. Rewriting `- [abc] title` to `- [Badge] title`
/// changes the node's KIND (Member to Object) and never its depth.
#[test]
fn annotation_changes_kind_not_depth() {
    let typed = parse("- [Card] Profile\n  - [abc] title\n  - [Badge] Status\n  - visible: true\n");
    let tagged = parse("- [Card] Profile\n  - [Badge] title\n  - [Badge] Status\n  - visible: true\n");

    let a = &typed.documents[0].nodes[0];
    let b = &tagged.documents[0].nodes[0];

    // Same parent, same three children, same order, same depth.
    assert_eq!(a.nodes.len(), 3);
    assert_eq!(b.nodes.len(), 3);
    assert_eq!(shape(a), shape(b), "the SHAPE must be identical");

    // Only the kind of the first child differs.
    assert_eq!(a.nodes[0].as_member().unwrap().sigil, Some(Sigil::String));
    assert_eq!(a.nodes[0].as_member().unwrap().name, "title");
    assert_eq!(b.nodes[0].as_object().unwrap().tag.as_deref(), Some("Badge"));
    assert_eq!(b.nodes[0].as_object().unwrap().head.render(), "title");
}

/// The same claim as a graph mutation: rewriting `kind` cannot reach `nodes`,
/// because they are different fields of the same struct.
#[test]
fn rewriting_kind_leaves_children_alone() {
    let mut stream = parse("- [Card] Profile\n  - [abc] title\n    - [abc] weight\n  - visible: true\n");
    let card = &mut stream.documents[0].nodes[0];
    let before = shape(card);

    let title = &mut card.nodes[0];
    assert!(title.as_member().is_some());
    assert_eq!(title.nodes.len(), 1, "title has one indented child");

    title.kind = NodeKind::Object(libtypedmd::Object {
        tag: Some("Badge".to_string()),
        head: libtypedmd::Text::literal("title"),
        sink: None,
        carrier: libtypedmd::Carrier::Row,
    });

    assert!(card.nodes[0].as_object().is_some());
    assert_eq!(card.nodes[0].nodes.len(), 1, "the child survived the kind change");
    assert_eq!(shape(card), before, "the shape is unchanged");
}

/// Members, sub-objects and leaves at the same indentation are SIBLINGS in
/// one ordered sequence under the same parent (spec §4.4).
#[test]
fn one_ordered_sequence_of_mixed_kinds() {
    let stream = parse(
        "- [Card] Profile\n  - [abc] title\n  - plain prose\n  - [Badge] Status\n  - visible: true\n",
    );
    let card = &stream.documents[0].nodes[0];
    let kinds: Vec<&str> = card.nodes.iter().map(kind_name).collect();
    assert_eq!(kinds, ["Member", "Leaf", "Object", "Member"]);
    assert!(card.nodes.iter().all(|n| n.nodes.is_empty()), "all four are at one depth");
}

/// Each level of list nesting is one level deeper, and nothing else moves a
/// node (spec §4.2).
#[test]
fn list_indentation_is_depth() {
    let stream = parse("- a: 1\n  - b: 2\n    - c: 3\n- d: 4\n");
    let nodes = &stream.documents[0].nodes;
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].as_member().unwrap().name, "a");
    assert_eq!(nodes[0].nodes[0].as_member().unwrap().name, "b");
    assert_eq!(nodes[0].nodes[0].nodes[0].as_member().unwrap().name, "c");
    assert!(nodes[0].nodes[0].nodes[0].nodes.is_empty());
    assert_eq!(nodes[1].as_member().unwrap().name, "d");
}

/// A heading's parent is the nearest open heading of LOWER level, and skipped
/// levels create no phantom nodes (spec §4.1).
#[test]
fn skipped_heading_levels_create_no_phantom_nodes() {
    let stream = parse("# A\n\n### C\n\n- x: 1\n");
    let nodes = &stream.documents[0].nodes;
    assert_eq!(nodes.len(), 1, "one root heading");
    assert_eq!(nodes[0].nodes.len(), 1, "`### C` is its only child, not a level-2 phantom");
    let c = &nodes[0].nodes[0];
    assert_eq!(c.as_object().unwrap().head.render(), "C");
    assert_eq!(c.nodes[0].as_member().unwrap().name, "x");
}

/// A heading closes all open list nesting and all open headings of equal or
/// higher level (spec §4.1).
#[test]
fn heading_closes_lists_and_equal_level_headings() {
    let stream = parse("# A\n\n- x: 1\n  - deep: 2\n\n## B\n\n- y: 3\n\n# C\n\n- z: 4\n");
    let roots = &stream.documents[0].nodes;
    assert_eq!(roots.len(), 2, "two level-1 headings at the root");

    let a = &roots[0];
    assert_eq!(a.nodes.len(), 2, "`x` and the `## B` heading");
    assert_eq!(a.nodes[0].as_member().unwrap().name, "x");
    assert_eq!(a.nodes[0].nodes[0].as_member().unwrap().name, "deep");
    let b = &a.nodes[1];
    assert_eq!(b.as_object().unwrap().head.render(), "B");
    assert_eq!(b.nodes.len(), 1, "`y` attached to B, not to the still-open list under A");

    assert_eq!(roots[1].as_object().unwrap().head.render(), "C");
}

/// A top-level list attaches to the innermost open heading (spec §4.2), and
/// composition is heading depth plus list nesting (spec §4.3).
#[test]
fn composition_of_heading_depth_and_list_nesting() {
    let stream = parse("# A\n\n## B\n\n- x: 1\n  - y: 2\n");
    let a = &stream.documents[0].nodes[0];
    let b = &a.nodes[0];
    let x = &b.nodes[0];
    assert_eq!(x.as_member().unwrap().name, "x");
    assert_eq!(x.nodes[0].as_member().unwrap().name, "y");
}

// -- helpers -----------------------------------------------------------------

/// The tree's SHAPE with every kind erased: child counts only.
fn shape(node: &Node) -> String {
    let inner: Vec<String> = node.nodes.iter().map(shape).collect();
    format!("({})", inner.join(""))
}

fn kind_name(node: &Node) -> &'static str {
    match &node.kind {
        NodeKind::Object(_) => "Object",
        NodeKind::Member(_) => "Member",
        NodeKind::Leaf(_) => "Leaf",
        NodeKind::Fallback(_) => "Fallback",
    }
}
