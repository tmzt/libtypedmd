//! Print the object graph a typed-Markdown file parses into.
//!
//! ```text
//! cargo run --example dump -- [--strict | --lenient] [FILE...]
//! ```
//!
//! With no files, reads standard input. Strict mode (the default) reports a
//! parse error and exits non-zero; lenient mode keeps going and shows what the
//! parser fell back on.
//!
//! The tree shows each node's kind and what the graph actually holds, which is
//! useful for checking how a document nests. A heading row with a colon, for
//! example, is a Member, and everything under it is its child.

use std::io::Read;
use std::process::ExitCode;

use libtypedmd::model::{Node, NodeKind, Text};
use libtypedmd::{parse_with, ParseOptions, Stream};

fn main() -> ExitCode {
    let mut options = ParseOptions::strict();
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--strict" => options = ParseOptions::strict(),
            "--lenient" => options = ParseOptions::lenient(),
            "-h" | "--help" => {
                eprintln!("usage: dump [--strict | --lenient] [FILE...]");
                return ExitCode::SUCCESS;
            }
            _ => files.push(arg),
        }
    }

    let mut failed = false;
    if files.is_empty() {
        let mut source = String::new();
        if let Err(error) = std::io::stdin().read_to_string(&mut source) {
            eprintln!("stdin: {error}");
            return ExitCode::FAILURE;
        }
        failed |= !dump("<stdin>", &source, &options);
    } else {
        for path in &files {
            match std::fs::read_to_string(path) {
                Ok(source) => failed |= !dump(path, &source, &options),
                Err(error) => {
                    eprintln!("{path}: {error}");
                    failed = true;
                }
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Print one file's graph. Returns whether it parsed.
fn dump(name: &str, source: &str, options: &ParseOptions) -> bool {
    println!("== {name}");
    match parse_with(source, options) {
        Ok(stream) => {
            print_stream(&stream);
            true
        }
        Err(error) => {
            println!("parse error: {error:?}");
            false
        }
    }
}

fn print_stream(stream: &Stream) {
    for (index, document) in stream.documents.iter().enumerate() {
        println!("document {index}");
        for (key, value) in document.envelope.entries() {
            println!("  envelope {key}: {value}");
        }
        for payload in &document.payloads {
            println!(
                "  payload ```{}``` ({} bytes)",
                payload.info,
                payload.body.len()
            );
        }
        for node in &document.nodes {
            print_node(node, 1);
        }
    }
}

fn print_node(node: &Node, depth: usize) {
    let pad = "  ".repeat(depth);
    match &node.kind {
        NodeKind::Object(object) => {
            let tag = object
                .tag
                .as_deref()
                .map(|t| format!("[{t}] "))
                .unwrap_or_default();
            let sink = object
                .sink
                .as_deref()
                .map(|s| format!(" -> {s}"))
                .unwrap_or_default();
            println!("{pad}object {tag}{}{sink}", text(&object.head));
        }
        NodeKind::Member(member) => {
            let sigil = member
                .sigil
                .map(|s| format!("[{}] ", s.token()))
                .unwrap_or_default();
            let rhs = member
                .rhs
                .as_ref()
                .map(|r| format!(" = {}", text(r)))
                .unwrap_or_default();
            let sink = member
                .sink
                .as_deref()
                .map(|s| format!(" -> {s}"))
                .unwrap_or_default();
            println!("{pad}member {sigil}{}{rhs}{sink}", member.name);
        }
        NodeKind::Leaf(leaf) => println!("{pad}leaf {}", quote(&leaf.text)),
        NodeKind::Fallback(fallback) => {
            println!(
                "{pad}fallback {} ({:?})",
                quote(&fallback.text),
                fallback.claim
            );
        }
    }
    for payload in &node.payloads {
        println!(
            "{pad}  payload ```{}``` ({} bytes)",
            payload.info,
            payload.body.len()
        );
    }
    for child in &node.nodes {
        print_node(child, depth + 1);
    }
}

/// A Text as written, slots included.
fn text(text: &Text) -> String {
    text.render()
}

/// Leaf text on one line, shortened so a paragraph doesn't swamp the tree.
fn quote(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = flat.chars().take(72).collect();
    if short.len() < flat.len() {
        format!("{short:?}...")
    } else {
        format!("{short:?}")
    }
}
