//! The lexical pre-pass, the envelope, and what a stream does with
//! restatements (spec §1.1, §6.1, Part II §5).

use libtypedmd::{emit, parse, prepass, Document, Envelope, Node, Sigil, Stream, Text};

/// A file with no structural `---` is ONE document with an empty envelope.
#[test]
fn no_separator_is_one_document() {
    let stream = parse("# [Record] User\n\n- [abc] id\n");
    assert_eq!(stream.documents.len(), 1);
    assert!(stream.documents[0].envelope.is_empty());
}

/// A structural `---` at document start opens frontmatter iff a closing `---`
/// arrives before any content (spec §6.1 rule 2).
#[test]
fn frontmatter_is_peeled_before_the_substrate_sees_anything() {
    let stream = parse("---\nbase: abc123\nfrom: somewhere\n---\n\n# [Record] A\n\n- [abc] id\n");
    let doc = &stream.documents[0];
    assert_eq!(doc.envelope.entries().len(), 2);
    assert_eq!(doc.envelope.get("base"), Some("abc123"));
    assert_eq!(doc.envelope.get("from"), Some("somewhere"));
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].as_object().unwrap().tag.as_deref(), Some("Record"));
}

/// The envelope is the EMISSION plane: flat `key: value`, values opaque, no
/// nesting (spec §1.1). It never becomes nodes.
#[test]
fn the_envelope_is_flat_and_opaque() {
    let stream = parse("---\nbase: a: b: c\nrev: 12 34\n---\n\n- x: 1\n");
    let env = &stream.documents[0].envelope;
    assert_eq!(env.get("base"), Some("a: b: c"));
    assert_eq!(env.get("rev"), Some("12 34"));
    assert_eq!(stream.documents[0].nodes.len(), 1, "only the body makes nodes");
}

/// Duplicate envelope keys: last occurrence wins, and the survivor keeps the
/// LAST position (spec §5).
#[test]
fn duplicate_envelope_keys_keep_the_last() {
    let stream = parse("---\nbase: one\nrev: 7\nbase: two\n---\n\n- x: 1\n");
    let env = &stream.documents[0].envelope;
    assert_eq!(env.entries(), &[("rev".to_string(), "7".to_string()), ("base".to_string(), "two".to_string())]);
}

/// A structural `---` after content closes the document and starts the next
/// (spec §6.1 rule 3).
#[test]
fn content_separator_splits_documents() {
    let stream = parse("- a: 1\n\n---\n\n- b: 2\n\n---\n\n- c: 3\n");
    assert_eq!(stream.documents.len(), 3);
    let names: Vec<&str> =
        stream.documents.iter().map(|d| d.nodes[0].as_member().unwrap().name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
}

/// A `---` that is NOT at file start and NOT after a blank line is not
/// structural (spec §6.1 rule 1).
#[test]
fn a_dash_rule_under_a_paragraph_is_not_a_split() {
    let stream = parse("Some prose\n---\nmore prose\n");
    assert_eq!(stream.documents.len(), 1, "a setext-shaped `---` does not split");
}

/// Inside a body a thematic break must be spelled `***` or `___`
/// (spec §6.1 rule 4). It carries no node.
#[test]
fn thematic_breaks_inside_a_body() {
    let stream = parse("- a: 1\n\n***\n\n- b: 2\n\n___\n\n- c: 3\n");
    assert_eq!(stream.documents.len(), 1);
    assert_eq!(stream.documents[0].nodes.len(), 3);
}

/// **The core never merges documents.** Every `---`-delimited document is
/// STORED as its own tree, in file order, including when two documents
/// restate the same object (Part II §5).
#[test]
fn restatements_are_stored_as_separate_trees() {
    let stream = parse(concat!(
        "# [Record] User\n",
        "\n",
        "- [abc] id\n",
        "\n",
        "---\n",
        "\n",
        "# [Record] User\n",
        "\n",
        "- [abc] email\n",
    ));

    assert_eq!(stream.documents.len(), 2, "two trees, not one merged one");
    for doc in &stream.documents {
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0].as_object().unwrap().head.render(), "User");
        assert_eq!(doc.nodes[0].nodes.len(), 1, "neither tree gained the other's member");
    }
    assert_eq!(stream.documents[0].nodes[0].nodes[0].as_member().unwrap().name, "id");
    assert_eq!(stream.documents[1].nodes[0].nodes[0].as_member().unwrap().name, "email");

    // And the split survives emission.
    assert_eq!(parse(&emit(&stream)), stream);
}

/// A stream built in Rust emits documents joined by a single `---` line, each
/// with its own envelope.
#[test]
fn emitting_a_multi_document_stream() {
    let doc = |key: &str, name: &str| {
        let mut envelope = Envelope::new();
        envelope.set("rev", key);
        Document {
            envelope,
            payloads: Vec::new(),
            nodes: vec![Node::typed(Sigil::String, name)],
        }
    };
    let stream = Stream::with(vec![doc("1", "id"), doc("2", "email")]);
    assert_eq!(
        emit(&stream),
        concat!(
            "---\n", "rev: 1\n", "---\n", "\n", "- [abc] id\n", "\n",
            "---\n", "\n",
            "---\n", "rev: 2\n", "---\n", "\n", "- [abc] email\n",
        )
    );
    assert_eq!(parse(&emit(&stream)), stream);
}

/// The pre-pass runs BEFORE the substrate: it hands on bodies and envelope
/// lines, never a document that still contains a structural `---`.
#[test]
fn the_prepass_hands_the_substrate_only_bodies() {
    let raw = prepass::split_stream("---\nk: v\n---\n\n- a: 1\n\n---\n\n- b: 2\n");
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].envelope_lines, vec![(2usize, "k: v".to_string())]);
    assert!(!raw[0].body.contains("---"), "body: {:?}", raw[0].body);
    assert!(!raw[1].body.contains("---"), "body: {:?}", raw[1].body);
    assert_eq!(raw[1].body_line, 9, "1-based file line of the second body");
}

/// Paragraph text is a VALUE and survives even when it looks like Markdown.
#[test]
fn paragraph_values_survive_looking_like_markdown() {
    let doc = Document::with_nodes(vec![Node::paragraph("- not a list\n--- not a split\n# not a heading")]);
    let stream = Stream::with(vec![doc]);
    let out = emit(&stream);
    assert!(out.contains("\\- not a list"), "escaped on the way out:\n{out}");
    assert_eq!(parse(&out), stream, "and unescaped on the way back");
}

/// Sanity: an empty input is one empty document, and emits as nothing.
#[test]
fn empty_input() {
    let stream = parse("");
    assert_eq!(stream.documents.len(), 1);
    assert_eq!(stream.documents[0], Document::new());
    assert_eq!(emit(&stream), "");
    assert_eq!(parse(&emit(&stream)), stream);
}

/// A slot in a head still round-trips when the document also has an envelope
/// and payloads, which is the shape most real files have.
#[test]
fn a_realistic_document() {
    let input = concat!(
        "---\n",
        "base: 9f2c\n",
        "---\n",
        "\n",
        "# [Screen] Inbox\n",
        "\n",
        "Intro prose.\n",
        "\n",
        "- [List] source={{ [1.] inbox.threads }} item_alias=\"thread\"\n",
        "  - [Text] {{ [abc] thread.subject }} -> thread.open\n",
        "  - [Card] Preview\n",
        "    - [abc] snippet\n",
        "    - visible: true\n",
        "\n",
        "## [Record] Thread\n",
        "\n",
        "- [abc] subject\n",
        "- [31] received\n",
    );
    let graph = parse(input);
    assert_eq!(parse(&emit(&graph)), graph);
    assert_eq!(emit(&parse(&emit(&graph))), emit(&graph));

    let screen = &graph.documents[0].nodes[0];
    assert_eq!(screen.nodes.len(), 3, "prose, the list, and the nested record heading");
    let list = &screen.nodes[1];
    assert_eq!(list.nodes.len(), 4, "two inline members and two children, one sequence");
    let text = &list.nodes[2];
    assert_eq!(text.as_object().unwrap().sink.as_deref(), Some("thread.open"));
    let _ = Text::empty();
}
