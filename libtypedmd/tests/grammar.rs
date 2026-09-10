//! The row grammar and the sigil table (spec §2, §3).

use libtypedmd::{emit_document, parse, Document, Node, Placement, Sigil, Text, TextPart, SIGILS};

fn first(input: &str) -> Node {
    parse(input).documents.remove(0).nodes.remove(0)
}

/// Exactly seven canonical ASCII sigils, recognized by EXACT TOKEN MATCH.
#[test]
fn the_seven_sigils() {
    let expected = [
        ("abc", Sigil::String),
        ("#", Sigil::Number),
        ("1.", Sigil::List),
        ("-", Sigil::Link),
        ("31", Sigil::Date),
        ("_", Sigil::Boolean),
        ("$", Sigil::Currency),
    ];
    assert_eq!(SIGILS.len(), 7);
    for (token, sigil) in expected {
        assert_eq!(Sigil::from_token(token), Some(sigil), "`[{token}]` is a sigil");
        assert_eq!(sigil.token(), token, "and emits as itself");
        let node = first(&format!("- [{token}] name\n"));
        assert_eq!(node.as_member().unwrap().sigil, Some(sigil));
        assert_eq!(node.as_member().unwrap().name, "name");
    }
}

/// No patterns: a token that merely LOOKS like a sigil is a tag.
#[test]
fn near_misses_are_tags_not_sigils() {
    for token in ["abcd", "ABC", "1", "1.2", "##", "32", "__", "$$", "Zorp", "123", "Screen"] {
        assert_eq!(Sigil::from_token(token), None, "`[{token}]` must not be a sigil");
        let node = first(&format!("- [{token}] head\n"));
        assert_eq!(
            node.as_object().unwrap().tag.as_deref(),
            Some(token),
            "`[{token}]` produces an Object carrying the token verbatim"
        );
    }
}

/// A sigil appears in exactly two positions and prefixes what it types in
/// both: leading a row, and leading a slot.
#[test]
fn a_sigil_types_a_row_and_a_slot() {
    let row = first("- [abc] id\n");
    assert_eq!(row.as_member().unwrap().sigil, Some(Sigil::String));

    let slotted = first("- [Text] {{ [abc] svc.name }}\n");
    let slot = match slotted.as_object().unwrap().head.parts() {
        [TextPart::Slot(s)] => s.clone(),
        other => panic!("expected a slot, got {other:?}"),
    };
    assert_eq!(slot.sigil, Some(Sigil::String));
    assert_eq!(slot.expr, "svc.name");
}

/// At most one annotation, recognized only as the FIRST token after the
/// marker. Later bracketed tokens are ordinary text (spec §3, rule 1).
#[test]
fn only_the_first_bracketed_token_is_an_annotation() {
    let node = first("- [Card] see [abc] and [#] below\n");
    assert_eq!(node.as_object().unwrap().tag.as_deref(), Some("Card"));
    assert_eq!(node.as_object().unwrap().head.render(), "see [abc] and [#] below");
}

/// `- name: value` and `name=value` produce the SAME node: one name, one RHS.
/// Only the placement flag differs, and consumers must ignore it.
#[test]
fn one_member_two_spellings() {
    let row = first("- [Card] X\n  - mode: wide\n");
    let inline = first("- [Card] X mode=wide\n");

    let a = row.nodes[0].as_member().unwrap();
    let b = inline.nodes[0].as_member().unwrap();
    assert_eq!(a.name, b.name);
    assert_eq!(a.rhs, b.rhs);
    assert_eq!(a.sigil, b.sigil);
    assert_eq!(a.sink, b.sink);
    assert_eq!(row.nodes[0].nodes, inline.nodes[0].nodes);
    assert_eq!((a.placement, b.placement), (Placement::Row, Placement::Inline));
}

/// A head may contain spaces and ends at the first of: an inline property, an
/// unquoted ` -> `, an unquoted `:` before whitespace, or end of line
/// (spec §3.1).
#[test]
fn head_delimitation() {
    // A colon NOT followed by whitespace does not end a head. This matters
    // in the HEAD position, not only in the RHS: the RHS is rest-of-line and
    // never splits on a colon, so a test that only puts the URL after the
    // colon passes even when the rule is gone.
    let url = first("- http://example.com/x: the value\n");
    assert_eq!(url.as_member().unwrap().name, "http://example.com/x");
    assert_eq!(url.as_member().unwrap().rhs.as_ref().unwrap().render(), "the value");

    let heading = first("# http://example.com/x\n");
    assert_eq!(heading.as_object().unwrap().head.render(), "http://example.com/x");
    assert_eq!(heading.as_object().unwrap().tag, None);

    // And in the RHS, where rest-of-line keeps it whole.
    let rhs = first("- link: http://example.com/x\n");
    assert_eq!(rhs.as_member().unwrap().name, "link");
    assert_eq!(rhs.as_member().unwrap().rhs.as_ref().unwrap().render(), "http://example.com/x");

    // An inline property ends the head.
    let props = first("- [Screen] Main Window mode=wide\n");
    assert_eq!(props.as_object().unwrap().head.render(), "Main Window");
    assert_eq!(props.nodes[0].as_member().unwrap().name, "mode");

    // A sink ends the head.
    let sunk = first("- [-] team member -> teams.id\n");
    assert_eq!(sunk.as_member().unwrap().name, "team member");
    assert_eq!(sunk.as_member().unwrap().sink.as_deref(), Some("teams.id"));

    // A head beginning with a quote runs to the closing quote, so it may
    // contain the terminators literally.
    let quoted = first("- [Card] \"Notes: draft -> x\" mode=wide\n");
    assert_eq!(quoted.as_object().unwrap().head.render(), "Notes: draft -> x");
    assert_eq!(quoted.nodes[0].as_member().unwrap().name, "mode");
    assert_eq!(quoted.as_object().unwrap().sink, None);
}

/// The sink, when present, is last on the row (spec §3, rule 5), in all three
/// positions it can occupy.
#[test]
fn sinks() {
    assert_eq!(first("- [Card] X -> a.b\n").as_object().unwrap().sink.as_deref(), Some("a.b"));
    assert_eq!(first("- [-] team -> teams.id\n").as_member().unwrap().sink.as_deref(), Some("teams.id"));

    let with_rhs = first("- name: value -> some.path\n");
    let m = with_rhs.as_member().unwrap();
    assert_eq!(m.rhs.as_ref().unwrap().render(), "value");
    assert_eq!(m.sink.as_deref(), Some("some.path"));

    let empty_rhs = first("- name: -> some.path\n");
    let m = empty_rhs.as_member().unwrap();
    assert_eq!(m.rhs, None);
    assert_eq!(m.sink.as_deref(), Some("some.path"));
}

/// A value is a bare token, a double-quoted string, or a slot (spec §3,
/// rule 4). All three produce one opaque RHS.
#[test]
fn inline_property_values() {
    let node = first("- [List] a=bare b=\"two words\" c={{ [#] x.y }} d=\"has \\\"quote\\\"\"\n");
    let value = |i: usize| node.nodes[i].as_member().unwrap().rhs.clone().unwrap();
    assert_eq!(value(0).as_literal(), Some("bare"));
    assert_eq!(value(1).as_literal(), Some("two words"));
    assert_eq!(value(2).as_slot().unwrap().expr, "x.y");
    assert_eq!(value(2).as_slot().unwrap().sigil, Some(Sigil::Number));
    assert_eq!(value(3).as_literal(), Some("has \"quote\""));
}

/// Slots must open and close on one line and never nest; the optional sigil
/// must be the first token inside the braces; `\{{` is a literal `{{`
/// (spec §3.2).
#[test]
fn slots() {
    // No sigil.
    let plain = first("- [Text] {{ a.b }}\n");
    assert_eq!(plain.as_object().unwrap().head.as_slot().unwrap().sigil, None);

    // A bracketed token that is NOT a sigil is part of the expression: slots
    // take types, not tags.
    let tagish = first("- [Text] {{ [Zorp] a.b }}\n");
    let slot = tagish.as_object().unwrap().head.as_slot().unwrap();
    assert_eq!(slot.sigil, None);
    assert_eq!(slot.expr, "[Zorp] a.b");

    // Mixed literal and slot text.
    let mixed = first("- [Text] before {{ [abc] x }} after\n");
    let parts = mixed.as_object().unwrap().head.parts();
    assert_eq!(parts.len(), 3);
    assert!(matches!(parts[1], TextPart::Slot(_)));

    // The escape, and the case that proves it is load-bearing: without a
    // closing `}}` an unescaped `{{` is literal anyway, so a test that omits
    // the `}}` passes even when the emitter stops escaping.
    let escaped = first("- [Text] literal \\{{ here\n");
    assert_eq!(escaped.as_object().unwrap().head.render(), "literal {{ here");

    let closed = first("- [Text] \\{{ x }} here\n");
    assert_eq!(closed.as_object().unwrap().head.parts().len(), 1, "one literal, not a slot");
    assert_eq!(closed.as_object().unwrap().head.render(), "{{ x }} here");

    let stream = parse("- [Text] \\{{ x }} here\n");
    let out = emit_document(&stream.documents[0]);
    assert_eq!(out, "- [Text] \\{{ x }} here\n", "the escape is re-emitted");
    assert_eq!(parse(&out), stream, "and without it the literal would become a slot");

    // Not recognized inside a fenced payload.
    let payload = parse("- [Module] m\n  ```\n  {{ [abc] not.a.slot }}\n  ```\n");
    let body = &payload.documents[0].nodes[0].payloads[0].body;
    assert_eq!(body, "{{ [abc] not.a.slot }}\n");

    // Nor in an envelope value.
    let env = parse("---\nbase: {{ [abc] not.a.slot }}\n---\n\n- x: 1\n");
    assert_eq!(env.documents[0].envelope.get("base"), Some("{{ [abc] not.a.slot }}"));
}

/// Row classification (spec §3, rule 2).
#[test]
fn row_classification() {
    assert!(first("- [abc] id\n").as_member().is_some(), "core sigil -> Member");
    assert!(first("- [Zorp] id\n").as_object().is_some(), "non-core tag -> Object");
    assert!(first("- name: value\n").as_member().is_some(), "colon-RHS -> Member");
    assert!(first("- just prose\n").as_leaf().is_some(), "no annotation, no colon -> Leaf");
    assert!(first("# Untitled\n").as_object().is_some(), "untagged heading -> Object");
    assert_eq!(first("# Untitled\n").as_object().unwrap().tag, None);
}

/// A payload keeps its info string verbatim and may repeat, in order
/// (spec §5).
#[test]
fn payloads_are_ordered_and_keep_their_info_strings() {
    let node = first("- [Module] m\n  ```rust ignore\n  one\n  ```\n  ```json\n  two\n  ```\n");
    assert_eq!(node.payloads.len(), 2);
    assert_eq!(node.payloads[0].info, "rust ignore");
    assert_eq!(node.payloads[0].body, "one\n");
    assert_eq!(node.payloads[1].info, "json");
    assert_eq!(node.payloads[1].body, "two\n");
}

/// A body's fence can hold backticks, and the emitter picks a fence that
/// survives it.
#[test]
fn payload_bodies_survive_their_own_fences() {
    let node = Node::object("Module", Text::literal("m"))
        .with_payload(libtypedmd::Payload::new("", "```\nnested\n```\n"));
    let doc = Document::with_nodes(vec![node]);
    let out = emit_document(&doc);
    assert!(out.contains("````"), "the fence grew to clear the body:\n{out}");
    assert_eq!(parse(&out), libtypedmd::Stream::with(vec![doc]));
}

/// Duplicate keys: within one plane on one node, the LAST occurrence wins and
/// earlier ones are dropped (spec §5). The two spellings share the plane.
#[test]
fn duplicate_members_keep_the_last() {
    let node = first("- [Card] X\n  - a: 1\n  - b: 2\n  - a: 3\n");
    assert_eq!(node.nodes.len(), 2);
    assert_eq!(node.nodes[0].as_member().unwrap().name, "b");
    assert_eq!(node.nodes[1].as_member().unwrap().name, "a");
    assert_eq!(node.nodes[1].as_member().unwrap().rhs.as_ref().unwrap().render(), "3");

    let crossed = first("- [Card] X a=1\n  - a: 2\n");
    assert_eq!(crossed.nodes.len(), 1, "an inline and a row member are one plane");
    assert_eq!(crossed.nodes[0].as_member().unwrap().rhs.as_ref().unwrap().render(), "2");
}

/// The RHS is opaque: the core records one right-hand side and never decides
/// whether it reads as a type or a value (Part II §3).
#[test]
fn the_rhs_is_opaque() {
    let schema = first("- active: Boolean\n");
    let widget = first("- active: true\n");
    assert_eq!(schema.as_member().unwrap().rhs.as_ref().unwrap(), &Text::literal("Boolean"));
    assert_eq!(widget.as_member().unwrap().rhs.as_ref().unwrap(), &Text::literal("true"));
}

/// A payload emits immediately after its node's own row, BEFORE that node's
/// children — otherwise a nested heading would be the innermost open heading
/// when the fence is read back, and the payload would change owner.
#[test]
fn a_payload_stays_on_the_node_it_was_attached_to() {
    let doc = Document::with_nodes(vec![Node::untagged(Text::literal("A"))
        .with_payload(libtypedmd::Payload::new("json", "{}\n"))
        .with_child(Node::untagged(Text::literal("B")))]);
    let out = emit_document(&doc);
    assert!(
        out.find("```json").unwrap() < out.find("## B").unwrap(),
        "the fence must precede the nested heading:\n{out}"
    );

    let stream = libtypedmd::Stream::with(vec![doc]);
    assert_eq!(parse(&out), stream, "and it comes back on `A`, not on `B`");
    assert!(parse(&out).documents[0].nodes[0].nodes[0].payloads.is_empty(), "not on B");
}

/// [`Text`] normalises on construction — adjacent literals merge, empty
/// literals drop — so two texts that render identically COMPARE equal. Graph
/// equality is over the fields in spec §1, and a text split differently is
/// not a different value.
#[test]
fn text_parts_are_normalised_so_equal_values_compare_equal() {
    use libtypedmd::TextPart;

    let split = Text::from_parts(vec![
        TextPart::Literal("ab".to_string()),
        TextPart::Literal("cd".to_string()),
    ]);
    assert_eq!(split, Text::literal("abcd"));
    assert_eq!(split.parts().len(), 1);

    let padded = Text::from_parts(vec![
        TextPart::Literal(String::new()),
        TextPart::Slot(libtypedmd::Slot::new("x")),
        TextPart::Literal(String::new()),
    ]);
    assert_eq!(padded, Text::slot(libtypedmd::Slot::new("x")));
    assert_eq!(padded.parts().len(), 1);

    assert_eq!(Text::from_parts(vec![TextPart::Literal(String::new())]), Text::empty());
    assert!(Text::empty().is_empty());
}

/// The emitter quotes EXACTLY where a terminator would otherwise bite, and
/// nowhere else. Quoting and scanning ask the same predicate of the same
/// string, so they cannot drift apart.
#[test]
fn emission_quotes_where_a_terminator_would_bite() {
    let cases = [
        // An untagged head: the colon is live, so it must be quoted.
        "# \"Notes: draft\"\n",
        // A tagged head: the colon is not live, but ` -> ` and `x=` are.
        "- [Card] \"a -> b\"\n",
        "- [Card] \"a x=y\"\n",
        // A member name.
        "- \"a: b\": v\n",
        "- [abc] \"x -> y\"\n",
        // An inline property value with whitespace.
        "- [List] label=\"two words\"\n",
        // A leaf whose value contains a colon.
        "- \"Note: this matters\"\n",
    ];
    for input in cases {
        let graph = parse(input);
        assert_eq!(emit_document(&graph.documents[0]), input, "for {input:?}");
        assert_eq!(parse(input), graph, "for {input:?}");
    }

    // And the same values WITHOUT their terminators keep their quotes off.
    assert_eq!(emit_document(&parse("# \"Notes draft\"\n").documents[0]), "# Notes draft\n");
    assert_eq!(
        emit_document(&parse("- [List] label=\"oneword\"\n").documents[0]),
        "- [List] label=oneword\n"
    );
}

/// Graph equality ignores locations (spec §8, guarantee 1), so the same node
/// read at two different lines is the same node.
#[test]
fn graph_equality_ignores_locations() {
    let early = parse("- [bad token] x\n");
    let late = parse("\n\n\n- [bad token] x\n");
    assert_eq!(early.documents[0].nodes, late.documents[0].nodes);
    assert_ne!(
        early.documents[0].nodes[0].as_fallback().unwrap().claim.location.line,
        late.documents[0].nodes[0].as_fallback().unwrap().claim.location.line,
        "the locations really do differ"
    );
}

/// Canonical form is deterministic down to where a payload sits inside a list
/// item, so the emitter has a fixed point to be checked against.
#[test]
fn a_list_item_payload_sits_between_the_row_and_its_children() {
    let doc = Document::with_nodes(vec![Node::object("Module", Text::literal("m"))
        .with_payload(libtypedmd::Payload::new("rust", "fn x() {}\n"))
        .with_child(Node::typed(Sigil::String, "name"))]);
    assert_eq!(
        emit_document(&doc),
        "- [Module] m\n  ```rust\n  fn x() {}\n  ```\n  - [abc] name\n"
    );
}
