//! Round-trip (spec §8).
//!
//! # Why these tests are shaped the way they are
//!
//! The ancestor of this library shipped a round-trip test that was a
//! `String::clone` assertion: its `to_markdown()` returned the source it had
//! stored on the document, so the test compared the input with itself and
//! proved nothing about the emitter. Every test in this file is written so
//! that an emitter which concatenated stored spans would FAIL it:
//!
//! * [`emission_follows_the_graph_not_the_source`] mutates the graph after
//!   parsing and requires the mutation in the output. An echo emitter emits
//!   the pre-mutation source.
//! * [`emission_is_not_echo`] parses a deliberately non-canonical spelling
//!   and requires the output to differ from the input in exactly the
//!   normalised syntax while preserving every value. An echo emitter returns
//!   the input byte for byte.
//! * [`emits_a_graph_that_was_never_parsed`] emits a graph built in Rust,
//!   which has no source to echo.
//! * [`the_graph_has_no_source_field`] fails to COMPILE if anyone adds one.

use libtypedmd::{
    emit, emit_document, parse, Carrier, Document, Envelope, Fallback, Leaf, Member, Node,
    NodeKind, Object, Payload, Placement, Sigil, Slot, Stream, Text,
};

/// Documents that between them exercise every field in spec §1.
const CORPUS: &[&str] = &[
    // Schema-shaped (spec §9).
    "# [Record] User\n\n- [abc] id\n- [abc] name\n- [#] age\n- [-] team -> teams.id\n- active: Boolean\n",
    // Widget-shaped, slots in a property value and in heads (spec §9).
    "# [Screen] Inbox\n\n- [List] source={{ [#] inbox.threads }} item_alias=\"thread\"\n  - [Text] {{ [abc] thread.subject }}\n  - [Text] {{ [abc] thread.sender }}\n",
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
];

/// Guarantee 1: for every graph `G`, `parse(emit(G)) == G` — graph equality
/// over every field in spec §1, including order, ignoring locations.
#[test]
fn semantic_round_trip() {
    for (i, source) in CORPUS.iter().enumerate() {
        let graph = parse(source);
        let again = parse(&emit(&graph));
        assert_eq!(again, graph, "corpus[{i}] did not survive parse -> emit -> parse:\n{source}");
    }
}

/// Guarantee 2: `emit(parse(emit(G))) == emit(G)`, byte for byte.
#[test]
fn emitter_fixed_point() {
    for (i, source) in CORPUS.iter().enumerate() {
        let once = emit(&parse(source));
        let twice = emit(&parse(&once));
        assert_eq!(twice, once, "corpus[{i}] is not an emitter fixed point");
    }
}

/// Guarantee 3: `emit(parse(input))` need NOT equal `input`. Byte equality of
/// arbitrary input is explicitly not the guarantee — and if it ever held for
/// the whole corpus, the emitter would be echoing.
#[test]
fn emission_is_not_byte_identity() {
    let differing = CORPUS.iter().filter(|s| emit(&parse(s)) != ***s).count();
    assert!(
        differing > 0,
        "no corpus entry was normalised; an echoing emitter would look exactly like this"
    );
}

/// **The sharp one.** Parse a deliberately NON-CANONICAL spelling and require
/// the emitted text to differ from the input in exactly the normalised
/// syntax, while preserving every value.
#[test]
fn emission_is_not_echo() {
    let input = concat!(
        "*   [Screen] Inbox\n",
        "    * [List] source={{[#] inbox.threads}} item_alias=\"thread\"\n",
        "        * [Text] {{[abc] thread.subject}}\n",
    );
    let graph = parse(input);
    let out = emit(&graph);

    // -- the syntax changed ------------------------------------------------
    assert_ne!(out, input, "an emitter that concatenated stored spans would pass here");
    assert!(!out.contains('*'), "`*` bullets normalise to `-`:\n{out}");
    assert!(out.starts_with("- [Screen] Inbox\n"), "one space after the marker:\n{out}");
    assert!(out.contains("\n  - [List] "), "two-space indent per list level:\n{out}");
    assert!(out.contains("\n    - [Text] "), "and again at the next level:\n{out}");
    assert!(out.contains("{{ [#] inbox.threads }}"), "slots get single padding:\n{out}");
    assert!(out.contains("{{ [abc] thread.subject }}"), "and so does the head slot:\n{out}");
    assert!(out.contains("item_alias=thread"), "a value needing no quotes loses them:\n{out}");

    // -- every value survived ----------------------------------------------
    let screen = &graph.documents[0].nodes[0];
    assert_eq!(screen.as_object().unwrap().tag.as_deref(), Some("Screen"));
    assert_eq!(screen.as_object().unwrap().head.render(), "Inbox");

    let list = &screen.nodes[0];
    assert_eq!(list.as_object().unwrap().tag.as_deref(), Some("List"));
    let source = list.nodes[0].as_member().unwrap();
    assert_eq!(source.name, "source");
    assert_eq!(source.placement, Placement::Inline);
    let slot = source.rhs.as_ref().unwrap().as_slot().unwrap();
    assert_eq!(slot.sigil, Some(Sigil::Number));
    assert_eq!(slot.expr, "inbox.threads");
    let alias = list.nodes[1].as_member().unwrap();
    assert_eq!(alias.name, "item_alias");
    assert_eq!(alias.rhs.as_ref().unwrap().as_literal(), Some("thread"));

    let text = &list.nodes[2];
    assert_eq!(text.as_object().unwrap().tag.as_deref(), Some("Text"));
    let head_slot = match text.as_object().unwrap().head.parts() {
        [libtypedmd::TextPart::Slot(s)] => s.clone(),
        other => panic!("expected one slot in the head, got {other:?}"),
    };
    assert_eq!(head_slot.sigil, Some(Sigil::String));
    assert_eq!(head_slot.expr, "thread.subject");

    // -- and the normalised text still means the same thing -----------------
    assert_eq!(parse(&out), graph);
}

/// Ordered markers are accepted and the ordinal is not preserved (spec §3,
/// rule 3), which no echoing emitter can reproduce.
#[test]
fn ordinals_are_not_preserved() {
    let out = emit(&parse("7. first\n8. [abc] id\n9) third\n"));
    assert_eq!(out, "- first\n- [abc] id\n- third\n");
}

/// **Emission follows the graph.** Mutate a value after parsing; the new
/// value must appear and the old must be gone. An emitter returning stored
/// source fails both assertions.
#[test]
fn emission_follows_the_graph_not_the_source() {
    let mut stream = parse("- [Card] Profile\n  - title: OLDVALUE\n  - [abc] name\n");

    let title = &mut stream.documents[0].nodes[0].nodes[0];
    match &mut title.kind {
        NodeKind::Member(m) => m.rhs = Some(Text::literal("NEWVALUE")),
        other => panic!("expected a member, got {other:?}"),
    }

    let out = emit(&stream);
    assert!(out.contains("title: NEWVALUE"), "the graph's value must reach the output:\n{out}");
    assert!(!out.contains("OLDVALUE"), "stored source must not reach the output:\n{out}");

    // Removing a node must remove its row, too.
    stream.documents[0].nodes[0].nodes.pop();
    let out = emit(&stream);
    assert!(!out.contains("[abc] name"), "a deleted node must not be echoed back:\n{out}");
}

/// A graph built in Rust — never parsed, so there is no source to echo.
#[test]
fn emits_a_graph_that_was_never_parsed() {
    let mut envelope = Envelope::new();
    envelope.set("base", "abc123");

    let record = Node::untagged(Text::literal("User"))
        .with_child(Node::typed(Sigil::String, "id"))
        .with_child(Node::typed(Sigil::Link, "team").with_sink("teams.id"))
        .with_child(Node::member("active").with_rhs(Text::literal("Boolean")))
        .with_payload(Payload::new("json", "{}\n"));

    let doc = Document { envelope, payloads: Vec::new(), nodes: vec![record] };
    let out = emit_document(&doc);

    assert_eq!(
        out,
        concat!(
            "---\n",
            "base: abc123\n",
            "---\n",
            "\n",
            "# User\n",
            "\n",
            "```json\n",
            "{}\n",
            "```\n",
            "\n",
            "- [abc] id\n",
            "- [-] team -> teams.id\n",
            "- active: Boolean\n",
        )
    );

    let stream = Stream::with(vec![doc]);
    assert_eq!(parse(&emit(&stream)), stream, "and it reads back as itself");
}

/// Nodes built programmatically default to row placement and row carrier
/// (spec §8.5, §8.6): `Placement::Inline` and `Carrier::Heading` are things a
/// parse OBSERVED, not defaults.
#[test]
fn programmatic_nodes_default_to_rows() {
    let node = Node::object("Card", Text::literal("Profile"))
        .with_child(Node::member("visible").with_rhs(Text::literal("true")));
    assert_eq!(node.as_object().unwrap().carrier, Carrier::Row);
    assert_eq!(node.nodes[0].as_member().unwrap().placement, Placement::Row);

    let doc = Document::with_nodes(vec![node]);
    assert_eq!(emit_document(&doc), "- [Card] Profile\n  - visible: true\n");
}

/// A member with `placement = Inline` emits on its parent's row only when it
/// can (spec §8.5); otherwise it demotes to a row, keeping its depth.
#[test]
fn inline_placement_demotes_rather_than_lying() {
    let inline = |name: &str, value: &str| {
        Node::member(name).with_rhs(Text::literal(value)).with_placement(Placement::Inline)
    };

    // Eligible: bare name, no sigil, no sink, no children, nothing before it.
    let ok = Node::object("List", Text::empty()).with_child(inline("mode", "wide"));
    assert_eq!(emit_document(&Document::with_nodes(vec![ok])), "- [List] mode=wide\n");

    // A non-inline sibling precedes it.
    let preceded = Node::object("List", Text::empty())
        .with_child(Node::member("first").with_rhs(Text::literal("1")))
        .with_child(inline("mode", "wide"));
    assert_eq!(
        emit_document(&Document::with_nodes(vec![preceded])),
        "- [List]\n  - first: 1\n  - mode: wide\n"
    );

    // A name that is not a bare identifier. Note that it needs no quotes as
    // a ROW: row-member names may contain spaces (spec §1.2). Only the inline
    // spelling requires a bare identifier.
    let spaced = Node::object("List", Text::empty()).with_child(inline("two words", "x"));
    assert_eq!(
        emit_document(&Document::with_nodes(vec![spaced])),
        "- [List]\n  - two words: x\n"
    );

    // It has children of its own.
    let parent = Node::object("List", Text::empty())
        .with_child(inline("mode", "wide").with_child(Node::member("deep")));
    assert_eq!(
        emit_document(&Document::with_nodes(vec![parent])),
        "- [List]\n  - mode: wide\n    - deep:\n"
    );

    // A member row has no inline properties at all (spec §3, `tail`).
    let member_parent = Node::member("outer").with_child(inline("mode", "wide"));
    assert_eq!(
        emit_document(&Document::with_nodes(vec![member_parent])),
        "- outer:\n  - mode: wide\n"
    );
}

/// A heading demotes to a list row when a row-carrier sibling follows it
/// (spec §8.6), because a heading would otherwise adopt that row.
#[test]
fn heading_carrier_demotes_when_a_row_follows() {
    let heading = || Node::object("Section", Text::literal("S")).with_carrier(Carrier::Heading);
    let row = || Node::object("Card", Text::literal("C"));

    let doc = Document::with_nodes(vec![heading(), row()]);
    assert_eq!(emit_document(&doc), "- [Section] S\n- [Card] C\n");
    assert_eq!(parse(&emit_document(&doc)).documents[0].nodes.len(), 2, "still two siblings");

    let doc = Document::with_nodes(vec![row(), heading()]);
    assert_eq!(emit_document(&doc), "- [Card] C\n\n# [Section] S\n");
    assert_eq!(parse(&emit_document(&doc)).documents[0].nodes.len(), 2);
}

/// Emission is structurally unable to return stored input, because the graph
/// has nowhere to put it.
///
/// This is a COMPILE-TIME assertion: an exhaustive destructuring pattern
/// stops compiling the moment a field is added. Adding `source: String` to
/// any of these types — the exact shape of the ancestor's bug — breaks the
/// build here rather than quietly making the round-trip test vacuous.
#[test]
fn the_graph_has_no_source_field() {
    fn stream(s: Stream) {
        let Stream { documents: _ } = s;
    }
    fn document(d: Document) {
        let Document { envelope: _, payloads: _, nodes: _ } = d;
    }
    fn node(n: Node) {
        let Node { kind: _, nodes: _, payloads: _ } = n;
    }
    fn object(o: Object) {
        let Object { tag: _, head: _, sink: _, carrier: _ } = o;
    }
    fn member(m: Member) {
        let Member { name: _, sigil: _, rhs: _, sink: _, placement: _ } = m;
    }
    fn leaf(l: Leaf) {
        let Leaf { text: _, form: _ } = l;
    }
    fn fallback(f: Fallback) {
        let Fallback { text: _, claim: _ } = f;
    }
    fn payload(p: Payload) {
        let Payload { info: _, body: _ } = p;
    }
    fn slot(s: Slot) {
        let Slot { sigil: _, expr: _ } = s;
    }

    stream(Stream::new());
    document(Document::new());
    node(Node::leaf("x"));
    object(Object::default());
    member(Member::default());
    leaf(Leaf::default());
    fallback(Fallback {
        text: String::new(),
        claim: libtypedmd::Claim::none(libtypedmd::Location::default()),
    });
    payload(Payload::default());
    slot(Slot::default());
}

/// A row leaf whose value contains a newline has no row spelling, so it
/// demotes to a paragraph. The `form` flag is lost — presentation — but the
/// VALUE survives, which a broken row would not.
#[test]
fn a_multiline_leaf_demotes_to_a_paragraph() {
    let doc = Document::with_nodes(vec![Node::leaf("a\nb")]);
    let out = emit_document(&doc);
    assert_eq!(out, "a\nb\n", "not a row:\n{out}");
    let back = parse(&out).documents.remove(0).nodes.remove(0);
    assert_eq!(back.as_leaf().unwrap().text, "a\nb", "the value survived");
    assert_eq!(back.as_leaf().unwrap().form, libtypedmd::LeafForm::Paragraph);
}
