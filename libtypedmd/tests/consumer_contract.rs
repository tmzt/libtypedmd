//! The consumer contract (Part II): the alias table, and the agnosticism gate.

use libtypedmd::{emit, parse_with, AliasTable, Annotation, ParseOptions, Sigil};

fn parse_aliased(input: &str, aliases: AliasTable) -> libtypedmd::Stream {
    parse_with(input, &ParseOptions::lenient().with_aliases(aliases)).expect("lenient")
}

/// Without a table, only the seven canonical sigils match and every other
/// bracketed token is carried verbatim (Part II §1).
#[test]
fn without_a_table_only_the_canonical_sigils_match() {
    let empty = AliasTable::new();
    assert!(empty.is_empty());
    assert_eq!(empty.resolve("abc"), Annotation::Sigil(Sigil::String));
    assert_eq!(empty.resolve("o"), Annotation::Tag("o".to_string()));
    assert_eq!(
        empty.resolve("Screen"),
        Annotation::Tag("Screen".to_string())
    );
}

/// The table covers BOTH sigils and tags.
#[test]
fn the_table_covers_sigils_and_tags() {
    let mut aliases = AliasTable::new();
    aliases.sigil("s", Sigil::String).tag("Panel", "Card");

    let stream = parse_aliased("- [s] id\n- [Panel] Profile\n", aliases);
    let nodes = &stream.documents[0].nodes;
    assert_eq!(nodes[0].as_member().unwrap().sigil, Some(Sigil::String));
    assert_eq!(nodes[1].as_object().unwrap().tag.as_deref(), Some("Card"));
}

/// **An alias never changes a node's kind**: a sigil alias resolves to a core
/// sigil (Member), a tag alias resolves to a tag (Object).
#[test]
fn an_alias_never_changes_a_nodes_kind() {
    let mut aliases = AliasTable::new();
    aliases.sigil("txt", Sigil::String).tag("txt-box", "Text");

    let stream = parse_aliased("- [txt] id\n- [txt-box] Hello\n", aliases);
    assert!(
        stream.documents[0].nodes[0].as_member().is_some(),
        "sigil alias -> Member"
    );
    assert!(
        stream.documents[0].nodes[1].as_object().is_some(),
        "tag alias -> Object"
    );
}

/// Canonical sigils are tested FIRST, so a table cannot shadow one and
/// silently change what an existing document means.
#[test]
fn a_table_cannot_shadow_a_canonical_sigil() {
    let mut aliases = AliasTable::new();
    aliases.sigil("abc", Sigil::Number).tag("#", "Heading");
    assert_eq!(aliases.resolve("abc"), Annotation::Sigil(Sigil::String));
    assert_eq!(aliases.resolve("#"), Annotation::Sigil(Sigil::Number));
}

/// Aliases resolve at PARSE time and the core emits canonical ASCII only
/// (spec §2, §8.3), so a legacy spelling is normalised on the way out.
#[test]
fn legacy_spellings_normalise_on_emission() {
    let mut aliases = AliasTable::new();
    aliases
        .sigil("o", Sigil::Link)
        .sigil("\u{29AF}", Sigil::String);

    let stream = parse_aliased("- [o] team -> teams.id\n- [\u{29AF}] name\n", aliases);
    let out = emit(&stream);
    assert_eq!(out, "- [-] team -> teams.id\n- [abc] name\n");
    assert!(
        !out.contains("[o]") && !out.contains('\u{29AF}'),
        "legacy spellings are gone:\n{out}"
    );
}

/// A slot's optional leading token is a SIGIL position, so the table's sigil
/// half applies there and its tag half does not.
#[test]
fn aliases_apply_inside_slots() {
    let mut aliases = AliasTable::new();
    aliases.sigil("s", Sigil::String).tag("Panel", "Card");

    let stream = parse_aliased(
        "- [Text] {{ [s] a.b }}\n- [Text] {{ [Panel] c.d }}\n",
        aliases,
    );
    let nodes = &stream.documents[0].nodes;
    let typed = nodes[0].as_object().unwrap().head.as_slot().unwrap();
    assert_eq!(typed.sigil, Some(Sigil::String));
    assert_eq!(typed.expr, "a.b");
    let untyped = nodes[1].as_object().unwrap().head.as_slot().unwrap();
    assert_eq!(untyped.sigil, None, "a tag alias is not a slot type");
    assert_eq!(untyped.expr, "[Panel] c.d");
}

/// **The agnosticism gate** (spec §10): the core crate forbids consumer
/// terminology.
///
/// This is mechanical on purpose. A stray consumer noun in a doc comment
/// reads as harmless and is exactly how a "generic" core acquires a customer;
/// nothing else in the suite can see it, because a term in a comment changes
/// no behaviour. Test names and this file are not scanned — only `src/`.
#[test]
fn the_core_names_no_consumer() {
    const FORBIDDEN: &[&str] = &[
        "tsx",
        "screen",
        "highbay",
        "storyboard",
        "widget",
        "prosemd",
        "cascada",
        "wgpu",
    ];

    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();
    let mut files = 0usize;
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src/ is readable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            files += 1;
            let text = std::fs::read_to_string(&path)
                .expect("utf-8 source")
                .to_lowercase();
            for term in FORBIDDEN {
                for (index, line) in text.lines().enumerate() {
                    if line.contains(term) {
                        offences.push(format!(
                            "{}:{}: `{term}`",
                            path.file_name().unwrap().to_string_lossy(),
                            index + 1
                        ));
                    }
                }
            }
        }
    }

    assert!(
        files >= 9,
        "the walk found only {files} source files; it is not looking at src/"
    );
    assert!(
        offences.is_empty(),
        "consumer terminology in the core:\n{}",
        offences.join("\n")
    );
}

/// The crate is STANDALONE: nothing it reads at test time lives outside its
/// own directory. This is the property that keeps it liftable — the ancestor
/// became unliftable by reaching for fixtures next door.
#[test]
fn the_crate_reads_nothing_outside_itself() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut stack = vec![manifest.join("src"), manifest.join("tests")];
    let mut checked = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            checked += 1;
            let text = std::fs::read_to_string(&path).expect("utf-8 source");
            // Spelled in pieces so this scanner does not trip over its own
            // source; it must scan itself like any other file.
            for needle in [
                concat!("include_", "str!"),
                concat!("include_", "bytes!"),
                concat!("..", "/"),
                concat!("std::env::", "var"),
            ] {
                assert!(
                    !text.contains(needle),
                    "{} reaches outside the crate via `{needle}`",
                    path.display()
                );
            }
        }
    }
    assert!(
        checked >= 10,
        "only {checked} files checked; the walk is wrong"
    );
}
