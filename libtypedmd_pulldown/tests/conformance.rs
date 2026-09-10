//! The second substrate, held to the first (spec §6.2).
//!
//! Every test here is one an ECHOING or STUBBED adapter would fail. The
//! ancestor of this library shipped a round-trip test that compared its input
//! with itself; the equivalent mistake at this seam would be an adapter that
//! delegated to `NativeParser`, or a check with nothing on the other side of
//! it. `libtypedmd`'s own `tests/substrate.rs` proves the check catches broken
//! substrates; this file puts a real one through it.

use libtypedmd::conformance::{self, DivergenceKind};
use libtypedmd::{emit, parse_with_substrate, Block, MarkdownParser, NativeParser, ParseOptions, Sigil};
use libtypedmd_pulldown::PulldownParser;

fn graph(input: &str) -> libtypedmd::Stream {
    parse_with_substrate(input, &ParseOptions::lenient(), &PulldownParser).expect("lenient")
}

/// **The test the trait existed for.** Both substrates, the same corpus, block
/// stream against block stream and graph against graph.
#[test]
fn the_two_substrates_agree_on_the_corpus() {
    if let Err(divergence) = conformance::check(&PulldownParser) {
        panic!("{divergence}");
    }
}

/// The corpus is the one in the LIBRARY. If it is ever forked, this fails.
#[test]
fn the_corpus_is_the_shared_one() {
    assert!(conformance::CORPUS.len() >= 16);
    assert!(
        conformance::CORPUS.iter().any(|d| d.contains("&amp;")),
        "the inline-noise document is what proves spans are sliced, not rebuilt"
    );
}

/// **Spans are sliced from the body, never rebuilt from events.** pulldown
/// resolves `&amp;` to `&`, drops backslash escapes, drops emphasis
/// delimiters, and reports `[d](e)` as a link. An adapter built from
/// `Event::Text` would return all five of those changed; the row's verbatim
/// source has none of them changed.
#[test]
fn spans_are_byte_exact_slices_of_the_body() {
    let body = "- [Text] a *b* `c` [d](e) &amp; \\*f\\*\n";
    let blocks = PulldownParser.parse_body(body);
    let source = match &blocks[..] {
        [Block::List { items, .. }] => items[0].source.clone(),
        other => panic!("expected one list, got {other:?}"),
    };
    assert_eq!(source.text, "[Text] a *b* `c` [d](e) &amp; \\*f\\*");
    assert_eq!(source.line, 1);
    assert_eq!(source.column, 3, "1-based byte column, past `- `");
    assert!(body.contains(&source.text), "the span is a substring of the body it came from");
}

/// **A sigil is never eaten as a CommonMark link.** With `[Card]: /somewhere`
/// in scope, pulldown reports `[Card]` as a shortcut LINK — and the row still
/// parses as an object tagged `Card`, because annotation recognition runs on
/// the verbatim source before any inline interpretation (spec §6.2).
#[test]
fn a_link_reference_definition_cannot_retype_a_row() {
    let stream = graph("[Card]: /somewhere\n\n- [Card] Profile\n- [abc] id\n");
    let nodes = &stream.documents[0].nodes;

    // The definition line is not a node's annotation and is not lost either:
    // pulldown reports no events at all for it, so it comes back off the
    // source as ordinary text.
    assert_eq!(nodes[0].as_leaf().expect("the definition survives as prose").text, "[Card]: /somewhere");
    assert_eq!(nodes[1].as_object().expect("still an object").tag.as_deref(), Some("Card"));
    assert_eq!(nodes[2].as_member().expect("still a member").sigil, Some(Sigil::String));
    assert_eq!(stream, parse_with_substrate("[Card]: /somewhere\n\n- [Card] Profile\n- [abc] id\n", &ParseOptions::lenient(), &NativeParser).unwrap());
}

/// The option set is EMPTY, pinned by value. Source-slicing makes this
/// adapter almost immune to the options — `[x]` stays in the row's text
/// whether or not task lists are on — so no output test can see this, and a
/// substrate that built spans from events would be eating annotations with
/// the same code passing the same suite.
#[test]
fn the_parser_options_are_empty() {
    assert_eq!(PulldownParser::OPTIONS, pulldown_cmark::Options::empty());
    assert!(!PulldownParser::OPTIONS.contains(pulldown_cmark::Options::ENABLE_TASKLISTS));
    assert!(!PulldownParser::OPTIONS.contains(pulldown_cmark::Options::ENABLE_TABLES));
    assert!(!PulldownParser::OPTIONS.contains(pulldown_cmark::Options::ENABLE_GFM));
}

/// **GFM is OFF.** `- [x] done` is GFM's task list and `[_]` is this format's
/// boolean sigil; a substrate with task lists on would eat the annotation.
/// `| a | b |` is a GFM table and the trait has no variant for one.
#[test]
fn gfm_is_off() {
    let stream = graph("| a | b |\n| --- | --- |\n\n- [_] done\n- [x] not a sigil\n");
    let nodes = &stream.documents[0].nodes;

    assert_eq!(nodes[0].as_leaf().expect("a table is prose").text, "| a | b |\n| --- | --- |");
    assert_eq!(nodes[1].as_member().expect("boolean").sigil, Some(Sigil::Boolean));
    assert_eq!(nodes[1].as_member().unwrap().name, "done");
    // `[x]` is not in the core table, so it is a consumer TAG, not a checkbox.
    assert_eq!(nodes[2].as_object().expect("a tag").tag.as_deref(), Some("x"));
    assert_eq!(nodes[2].as_object().unwrap().head.render(), "not a sigil");
}

/// Payload bodies are literal. pulldown decodes the text inside a fence too;
/// slicing does not.
#[test]
fn payload_bodies_are_literal() {
    let stream = graph("- [Module] m\n  ```html\n  <b>&amp;</b>\n  ```\n");
    let payload = &stream.documents[0].nodes[0].payloads[0];
    assert_eq!(payload.info, "html");
    assert_eq!(payload.body, "<b>&amp;</b>\n", "the entity is not resolved");
}

/// Adjacent lists of the same orderedness are ONE list. CommonMark starts a
/// new list when the bullet or the delimiter changes; the trait's `List`
/// carries neither, so reporting two would be a difference it cannot mean.
#[test]
fn a_changed_bullet_does_not_split_a_list() {
    for body in ["- a\n* b\n+ c\n", "1. a\n2) b\n"] {
        let blocks = PulldownParser.parse_body(body);
        match &blocks[..] {
            [Block::List { items, .. }] => assert_eq!(items.len(), body.lines().count()),
            other => panic!("{body:?} should be one list, got {other:?}"),
        }
        assert_eq!(blocks, NativeParser.parse_body(body));
    }
}

/// The emitter's output re-parses to the same graph under EITHER substrate,
/// which is what makes them interchangeable in practice rather than on paper.
#[test]
fn emission_round_trips_through_the_pulldown_substrate() {
    for (index, source) in conformance::CORPUS.iter().enumerate() {
        let once = graph(source);
        let again = graph(&emit(&once));
        assert_eq!(again, once, "corpus[{index}] did not survive parse -> emit -> parse");
        assert_eq!(emit(&graph(&emit(&once))), emit(&once), "corpus[{index}] is not a fixed point");
    }
}

// -- out-of-scope constructs -------------------------------------------------

/// **The blockquote question, measured.** `Block` has no `BlockQuote` variant
/// and the graph has no block-quote kind, so a quote is read as what the
/// native substrate has always read it as: verbatim source lines, joined into
/// the surrounding text run. The marker stays in the text, and a leaf's text
/// is its VALUE, so the content round-trips.
#[test]
fn a_block_quote_is_verbatim_text_under_both_substrates() {
    for body in [
        "> quoted text\n",
        "> a\n> b\n",
        "> - a\n> - b\n",
        "text line\n> quoted\n",
        "# H\n\n> quoted\n",
    ] {
        assert_eq!(
            PulldownParser.parse_body(body),
            NativeParser.parse_body(body),
            "the two substrates must read {body:?} the same way"
        );
    }

    let stream = graph("> quoted text\n");
    let leaf = stream.documents[0].nodes[0].as_leaf().expect("a quote is a leaf");
    assert_eq!(leaf.text, "> quoted text", "the marker is part of the value");
    assert_eq!(emit(&stream), "> quoted text\n", "and it comes back unchanged");
}

/// The same for the other constructs the trait has no variant for.
#[test]
fn tables_html_and_setext_are_verbatim_text_under_both_substrates() {
    for body in [
        // A GFM table with GFM off: three ordinary lines.
        "| a | b |\n| --- | --- |\n| 1 | 2 |\n",
        // An HTML block, whole and self-contained.
        "<div>\nstuff\n</div>\n",
        "text\n<span>x</span>\n",
        // Setext headings: this format has no setext spelling. `---` is the
        // stream separator and the emitter only writes ATX.
        "Title\n===\n",
        "Title\n---\n",
        // An indented code block.
        "para\n\n    code here\n",
        // A link reference definition, which pulldown reports as nothing.
        "[Card]: /x\n\n- [Card] P\n",
        "[Card]: /x\ntext right after\n",
    ] {
        assert_eq!(
            PulldownParser.parse_body(body),
            NativeParser.parse_body(body),
            "the two substrates must read {body:?} the same way"
        );
    }
}

/// **The boundary, pinned.** Three inputs where the two substrates genuinely
/// differ, each because CommonMark's EXTENT for an out-of-scope construct is
/// not something a five-shape line reader can reproduce. They are pinned so
/// that the list cannot grow silently, and so that a future fix to either
/// reader shows up here as a failing test rather than as a surprise.
///
/// See `SPEC.md` §6.2, "Out of scope".
#[test]
fn the_known_boundary_is_exactly_these_three() {
    // 1. An HTML block runs to a blank line, swallowing lines the native
    //    reader takes as blocks of their own.
    let body = "<div>\n## Heading\n";
    assert_ne!(PulldownParser.parse_body(body), NativeParser.parse_body(body));
    assert!(matches!(NativeParser.parse_body(body)[1], Block::Heading { .. }));
    assert_eq!(PulldownParser.parse_body(body).len(), 1, "one run of text");

    // 2. An ordered list whose first number is not 1 cannot interrupt a
    //    paragraph in CommonMark; the native reader starts a list at any
    //    marker.
    let body = "text\n2) two\n";
    assert_ne!(PulldownParser.parse_body(body), NativeParser.parse_body(body));
    assert_eq!(NativeParser.parse_body(body).len(), 2, "paragraph then list");
    assert_eq!(PulldownParser.parse_body(body).len(), 1, "one run of text");

    // 3. A TAB is four columns of indentation to CommonMark and no
    //    indentation at all to the native reader, which counts spaces.
    let body = "- a\n\t- tabbed\n";
    assert_ne!(PulldownParser.parse_body(body), NativeParser.parse_body(body));

    // And nothing is LOST in any of them: the content of every source line
    // reaches the block stream under both readers, only in a different shape.
    let body = "<div>\n## Heading\n";
    match &NativeParser.parse_body(body)[..] {
        [Block::Paragraph { lines }, Block::Heading { level, source }] => {
            assert_eq!(lines[0].text, "<div>");
            assert_eq!((*level, source.text.as_str()), (2, "Heading"));
        }
        other => panic!("{other:?}"),
    }
    match &PulldownParser.parse_body(body)[..] {
        [Block::Paragraph { lines }] => {
            assert_eq!(lines[0].text, "<div>");
            assert_eq!(lines[1].text, "## Heading", "the whole line, markers and all");
        }
        other => panic!("{other:?}"),
    }
}

/// A generated differential sweep: every ordered pair and triple of a set of
/// line shapes, run through both substrates. This is the test that FOUND the
/// boundary above — and two defects in the native reader — rather than one
/// written to confirm what was already believed.
///
/// Bodies containing an out-of-scope construct are excluded by the same three
/// rules the boundary test pins. Everything else must agree exactly.
#[test]
fn a_generated_sweep_finds_no_other_divergence() {
    const FRAGMENTS: &[&str] = &[
        "# [Record] User",
        "## Sub",
        "- [abc] id",
        "  - [#] n",
        "- plain row",
        "text line",
        "",
        "***",
        "---",
        "===",
        "```rust",
        "fn x() {}",
        "```",
        "    indented code",
        "[Card]: /x",
        "> quoted",
        "> - [abc] q",
        "1. ordered",
        "* star bullet",
        "- [Card] X -> a.b",
        "  continuation",
        "\\- escaped",
        "  ",
    ];

    let out_of_scope = |body: &str| {
        body.contains('<')
            || body.contains('\t')
            || body.lines().any(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with(|c: char| c.is_ascii_digit())
                    && !trimmed.starts_with("1.")
                    && !trimmed.starts_with("1)")
                    && trimmed.split_once(['.', ')']).is_some()
            })
    };

    let mut checked = 0usize;
    let mut divergent: Vec<String> = Vec::new();
    for a in FRAGMENTS {
        for b in FRAGMENTS {
            for c in FRAGMENTS {
                let body = format!("{a}\n{b}\n{c}\n");
                if out_of_scope(&body) {
                    continue;
                }
                checked += 1;
                if PulldownParser.parse_body(&body) != NativeParser.parse_body(&body) {
                    divergent.push(body);
                }
            }
        }
    }

    assert!(checked > 7000, "the sweep only ran {checked} bodies; it is not sweeping");
    assert!(
        divergent.is_empty(),
        "{} of {checked} bodies diverged, first five:\n{}",
        divergent.len(),
        divergent
            .iter()
            .take(5)
            .map(|body| format!(
                "{body:?}\n  native  : {:?}\n  pulldown: {:?}",
                NativeParser.parse_body(body),
                PulldownParser.parse_body(body)
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The sweep's own exclusion rule must not be doing the work: a body that the
/// rule admits and that WOULD diverge has to be caught. Break the adapter and
/// the sweep goes red.
#[test]
fn the_sweep_admits_bodies_that_can_diverge() {
    let body = "# [Record] User\n- [abc] id\ntext line\n";
    assert!(!body.contains('<') && !body.contains('\t'));
    let mut blocks = PulldownParser.parse_body(body);
    blocks.pop();
    assert_ne!(blocks, NativeParser.parse_body(body), "a dropped block must show up");
}

// -- the generated report ----------------------------------------------------

/// Write `REPORT.gen.md`: every corpus document, its input, and what the
/// emitter produces from the graph each substrate parsed it into. Generated,
/// never hand-edited, and deterministic — [`the_report_is_deterministic`]
/// checks that.
#[test]
fn writes_the_generated_report() {
    let path = report_path();
    std::fs::write(&path, render_report()).expect("REPORT.gen.md is writable");
    let written = std::fs::read_to_string(&path).expect("readable");
    assert!(written.contains("## 0."), "the report has entries");
    assert!(written.contains("````markdown"), "and they are fenced");
}

#[test]
fn the_report_is_deterministic() {
    assert_eq!(render_report(), render_report(), "the report has no clock and no RNG");
}

fn report_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .join("REPORT.gen.md")
}

/// A fence long enough to survive its own content: the corpus contains fences
/// of its own, so three backticks would let the payload escape. (The emitter
/// computes the same length for the same reason, privately — see
/// `libtypedmd::emit`. Two callers, one rule, neither reachable from the
/// other.)
fn fence_for(content: &str) -> String {
    let longest = content
        .lines()
        .map(|line| line.trim_start().chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

fn render_report() -> String {
    let mut out = String::new();
    out.push_str("# libtypedmd corpus: before and after\n\n");
    out.push_str("Generated by `cargo test -p libtypedmd_pulldown`. Do not edit.\n\n");
    out.push_str("Each entry is the corpus document, then `emit(parse(document))`. ");
    out.push_str("Where the two substrates disagree, both are shown.\n");

    for (index, source) in conformance::CORPUS.iter().enumerate() {
        let native = emit(&parse_with_substrate(source, &ParseOptions::lenient(), &NativeParser).expect("lenient"));
        let pulldown = emit(&graph(source));

        out.push_str(&format!("\n## {index}. before\n\n"));
        out.push_str(&block(source));
        if native == pulldown {
            out.push_str(&format!("\n## {index}. after\n\n"));
            out.push_str(&block(&native));
        } else {
            out.push_str(&format!("\n## {index}. after (native)\n\n"));
            out.push_str(&block(&native));
            out.push_str(&format!("\n## {index}. after (pulldown)\n\n"));
            out.push_str(&block(&pulldown));
        }
    }
    out
}

fn block(content: &str) -> String {
    let fence = fence_for(content);
    let mut out = String::with_capacity(content.len() + 32);
    out.push_str(&fence);
    out.push_str("markdown\n");
    out.push_str(content);
    if !content.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&fence);
    out.push('\n');
    out
}

/// The report must not be able to hide a divergence: if the two substrates
/// ever differ on a corpus document, the entry grows a second block.
#[test]
fn the_report_shows_both_substrates_when_they_differ() {
    let source = "<div>\n## Heading\n";
    let native = emit(&parse_with_substrate(source, &ParseOptions::lenient(), &NativeParser).unwrap());
    let pulldown = emit(&graph(source));
    assert_ne!(native, pulldown, "this is the boundary case; if it agrees, the pin below is vacuous");

    // Same shape as `render_report` uses, on a document that does diverge.
    assert!(block(&native) != block(&pulldown));
    // And the corpus itself does NOT diverge, which is why the report has one
    // block per entry today.
    assert_eq!(conformance::divergences(&PulldownParser).len(), 0);
    assert!(!render_report().contains("(pulldown)"));
}

/// Fences in the report survive their own content.
#[test]
fn report_fences_survive_their_own_content() {
    let with_fence = conformance::CORPUS
        .iter()
        .find(|document| document.contains("```"))
        .expect("the corpus has a payload with a fence");
    let rendered = block(with_fence);
    assert!(rendered.starts_with("````markdown\n"), "a longer fence:\n{rendered}");
    assert!(rendered.ends_with("````\n"));
    // The inner fence is intact, not escaped or shortened.
    assert!(rendered.contains("```rust\n"));
}

/// Every kind of divergence the corpus check can report is a kind this file
/// would notice.
#[test]
fn both_divergence_kinds_are_reachable() {
    struct DropsLast;
    impl MarkdownParser for DropsLast {
        fn parse_body(&self, body: &str) -> Vec<Block> {
            let mut blocks = NativeParser.parse_body(body);
            blocks.pop();
            blocks
        }
    }
    let kinds: Vec<DivergenceKind> =
        conformance::divergences(&DropsLast).into_iter().map(|d| d.kind).collect();
    assert!(kinds.contains(&DivergenceKind::Blocks));
    assert!(kinds.contains(&DivergenceKind::Graph));
}
