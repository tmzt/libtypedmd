//! The differential sweep, as a runnable tool.
//!
//! `cargo run -p libtypedmd_pulldown --example sweep` builds every single,
//! ordered pair and ordered triple of the line shapes below and reads each one
//! with both substrates. It is how the boundary in `SPEC.md` §6.2 was
//! measured, and how two defects in the native reader were found; the test
//! suite carries a narrower version of it (`a_generated_sweep_finds_no_other_
//! divergence`) that must stay at zero.
//!
//! Pass `--in-grammar` to hide the bodies that contain an out-of-scope
//! construct, which is the set the spec says may differ.

use libtypedmd::{parse_with_substrate, MarkdownParser, NativeParser, ParseOptions};
use libtypedmd_pulldown::PulldownParser;

const FRAGMENTS: &[&str] = &[
    "# [Record] User",
    "## Sub",
    "- [abc] id",
    "  - [#] n",
    "- plain row",
    "text line",
    "",
    "> quoted",
    "> - [abc] q",
    "| a | b |",
    "| --- | --- |",
    "<div>",
    "</div>",
    "<span>x</span>",
    "***",
    "---",
    "___",
    "===",
    "```rust",
    "fn x() {}",
    "```",
    "    indented code",
    "[Card]: /x",
    "1. ordered",
    "2) other delim",
    "* star bullet",
    "+ plus bullet",
    "\t- tab indented",
    "- [Card] X -> a.b",
    "  continuation",
    "Setext",
    "\\- escaped",
    "  ",
];

fn bodies() -> Vec<String> {
    let mut out = Vec::new();
    for a in FRAGMENTS {
        out.push(format!("{a}\n"));
        for b in FRAGMENTS {
            out.push(format!("{a}\n{b}\n"));
            for c in FRAGMENTS {
                out.push(format!("{a}\n{b}\n{c}\n"));
            }
        }
    }
    out
}

fn main() {
    let mut blocks_diff = 0usize;
    let mut graph_diff: Vec<String> = Vec::new();
    let all = bodies();
    for body in &all {
        let n = NativeParser.parse_body(body);
        let p = PulldownParser.parse_body(body);
        if n != p {
            blocks_diff += 1;
        }
        let ng = parse_with_substrate(body, &ParseOptions::lenient(), &NativeParser).unwrap();
        let pg = parse_with_substrate(body, &ParseOptions::lenient(), &PulldownParser).unwrap();
        if ng != pg {
            graph_diff.push(body.clone());
        }
    }
    println!(
        "bodies: {}  block-diffs: {}  graph-diffs: {}",
        all.len(),
        blocks_diff,
        graph_diff.len()
    );
    let mut short: Vec<&String> = graph_diff.iter().collect();
    short.sort_by_key(|b| {
        (
            b.lines().filter(|l| !l.trim().is_empty()).count(),
            b.len(),
            b.to_string(),
        )
    });
    let mut shown = 0;
    for body in &short {
        if std::env::args().any(|a| a == "--in-grammar")
            && (body.contains('<')
                || body.contains('\t')
                || body.lines().any(|l| l.starts_with("2)")))
        {
            continue;
        }
        if shown >= 30 {
            break;
        }
        shown += 1;
        println!("\n== {body:?}");
        if NativeParser.parse_body(body) == PulldownParser.parse_body(body) {
            for raw in libtypedmd::prepass::split_stream(body) {
                if NativeParser.parse_body(&raw.body) != PulldownParser.parse_body(&raw.body) {
                    println!("  sub-body {:?}", raw.body);
                    println!("  native  : {:?}", NativeParser.parse_body(&raw.body));
                    println!("  pulldown: {:?}", PulldownParser.parse_body(&raw.body));
                }
            }
        } else {
            println!("  native  : {:?}", NativeParser.parse_body(body));
            println!("  pulldown: {:?}", PulldownParser.parse_body(body));
        }
    }
}
