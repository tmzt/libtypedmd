//! Leniency, claims and strict mode (spec §7).
//!
//! Leniency is the DEFAULT. The producer is an LLM and will not always be
//! compliant. A lenient parse never fails on content: anything unrecognized
//! becomes a Fallback carrying its verbatim span plus a claim record.

use libtypedmd::{parse, parse_with, ClaimShape, ParseOptions};

fn first_kind(input: &str) -> libtypedmd::NodeKind {
    parse(input).documents.remove(0).nodes.remove(0).kind
}

/// A row that led with a bracketed token it could not resolve falls back with
/// `shape = Sigil`, keeping its span verbatim.
#[test]
fn unresolvable_annotation_falls_back_with_a_sigil_claim() {
    for (input, token) in [
        ("- [abc def] weird\n", "abc def"),
        ("- [] empty\n", ""),
        ("- [unclosed head\n", "unclosed head"),
        ("- [abc]nospace\n", "abc"),
    ] {
        let node = parse(input).documents.remove(0).nodes.remove(0);
        let fallback = node
            .as_fallback()
            .unwrap_or_else(|| panic!("{input:?} should fall back"));
        assert_eq!(fallback.claim.shape, ClaimShape::Sigil);
        assert_eq!(fallback.claim.token.as_deref(), Some(token));
        assert_eq!(fallback.text, input.trim_start_matches("- ").trim_end());
    }
}

/// A row that offered `name=` pairs it could not parse falls back with
/// `shape = Attrs`, recording the pairs it did scan, in order.
#[test]
fn unparseable_attributes_fall_back_with_an_attrs_claim() {
    let node = parse("- [List] mode=wide source=\n")
        .documents
        .remove(0)
        .nodes
        .remove(0);
    let fallback = node.as_fallback().expect("should fall back");
    assert_eq!(fallback.claim.shape, ClaimShape::Attrs);
    assert_eq!(fallback.claim.token.as_deref(), Some("List"));
    assert_eq!(
        fallback.claim.attrs,
        vec![
            ("mode".to_string(), "wide".to_string()),
            ("source".to_string(), String::new())
        ]
    );
}

/// Unannotated prose CLAIMS NOTHING and is preserved in both modes. It is
/// never a fallback, so it can never be a strict-mode error.
#[test]
fn prose_claims_nothing() {
    for input in [
        "- just a plain row\n",
        "Just a paragraph.\n",
        "- a row -> with an arrow\n",
    ] {
        let kind = first_kind(input);
        assert!(
            matches!(kind, libtypedmd::NodeKind::Leaf(_)),
            "{input:?} must be a Leaf, got {kind:?}"
        );
        assert!(
            parse_with(input, &ParseOptions::strict()).is_ok(),
            "{input:?} is strict-clean"
        );
    }
}

/// The strict predicate: the parse fails IFF a node fell back with
/// `shape != None`.
#[test]
fn strict_rejects_exactly_the_rows_that_claimed_something() {
    let bad = "# [Record] User\n\n- [abc] id\n- [abc def] weird\n";
    let err = parse_with(bad, &ParseOptions::strict()).expect_err("strict must reject");
    assert_eq!(err.location.document, 0);
    assert_eq!(err.location.line, 4, "the offending row's line");
    assert_eq!(err.location.column, 3, "1-based byte column, past `- `");
    let claim = err
        .claim
        .expect("the error carries the offending claim record");
    assert_eq!(claim.shape, ClaimShape::Sigil);
    assert_eq!(claim.token.as_deref(), Some("abc def"));

    // And the lenient parse of the same input returns a graph regardless.
    assert_eq!(parse(bad).documents[0].nodes[0].nodes.len(), 2);
}

/// Strict mode returns the FIRST error, in document order.
#[test]
fn strict_reports_the_first_error() {
    let err = parse_with("- [one bad] a\n- [two bad] b\n", &ParseOptions::strict())
        .expect_err("strict must reject");
    assert_eq!(err.location.line, 1);
    assert_eq!(err.claim.unwrap().token.as_deref(), Some("one bad"));
}

/// A well-formed document parses IDENTICALLY under both modes (spec §7).
#[test]
fn well_formed_documents_parse_the_same_in_both_modes() {
    let good =
        "# [Record] User\n\n- [abc] id\n- [-] team -> teams.id\n- active: Boolean\n\nProse.\n";
    let lenient = parse(good);
    let strict = parse_with(good, &ParseOptions::strict()).expect("strict must accept");
    assert_eq!(lenient, strict);
}

/// An unclosed `{{` is literal text in lenient mode and an error in strict
/// mode (spec §3.2).
#[test]
fn unclosed_slot() {
    let node = parse("- [Text] {{ oops\n")
        .documents
        .remove(0)
        .nodes
        .remove(0);
    assert_eq!(node.as_object().unwrap().head.render(), "{{ oops");
    let err = parse_with("- [Text] {{ oops\n", &ParseOptions::strict()).expect_err("strict");
    assert!(err.message.contains("unclosed slot"), "{}", err.message);
}

/// Strict mode rejects a duplicate at the SECOND occurrence's location
/// (spec §5).
#[test]
fn strict_rejects_duplicates_at_the_second_occurrence() {
    let err = parse_with("- [Card] X\n  - a: 1\n  - a: 2\n", &ParseOptions::strict())
        .expect_err("strict must reject the duplicate");
    assert!(
        err.message.contains("duplicate member name `a`"),
        "{}",
        err.message
    );
    assert_eq!(err.location.line, 3, "the second occurrence, not the first");
}

/// A lenient parse returns a graph unconditionally; nothing in the content
/// can make it fail.
#[test]
fn lenient_never_fails() {
    let hostile = concat!(
        "- [abc def] weird\n",
        "- [List] source=\n",
        "- [unclosed\n",
        "- {{ never closed\n",
        "  - [31] still nested\n",
        "\n",
        "```\n",
        "an unterminated fence\n",
    );
    let stream = parse(hostile);
    assert_eq!(stream.documents.len(), 1);
    assert_eq!(stream.documents[0].nodes.len(), 4);
    // The nested row is still nested: nothing above it changed its depth.
    assert_eq!(stream.documents[0].nodes[3].nodes.len(), 1);
    assert_eq!(
        stream.documents[0].nodes[3].nodes[0]
            .as_member()
            .unwrap()
            .sigil,
        Some(libtypedmd::Sigil::Date)
    );
}

/// A fallback keeps its span verbatim, so it survives a round trip unchanged
/// — including the claim it made.
#[test]
fn fallbacks_round_trip() {
    let input = "- [abc def] weird\n- [List] mode=wide source=\n";
    let graph = parse(input);
    assert_eq!(parse(&libtypedmd::emit(&graph)), graph);
}
