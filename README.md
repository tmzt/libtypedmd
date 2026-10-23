# libtypedmd

A generic object-graph format whose serialization is Markdown. The library
parses Markdown into a typed object graph and emits Markdown from that
graph; `- property: value` is YAML encoded as Markdown. A list becomes a
mapping, indentation is nesting, and one key has one right-hand side.

The core knows nothing about what a document means. It records structure:
objects, members, leaves and fallbacks, with bracketed tags and type sigils
carried as written. A consumer supplies the meaning: what a tag stands for,
whether a member's right-hand side is a type or a value, and how sink paths
and slot expressions resolve. The format is specified in [SPEC.md](SPEC.md).

## What it is for

Structured data that people and language models both read and write as
ordinary Markdown. The producer is often an LLM that will not always follow
the grammar, so parsing is lenient by default: anything unrecognised becomes
a `Fallback` node that keeps its verbatim text and a claim record saying what
it looked like it was trying to be. An opt-in strict mode rejects a row that
offered a sigil or attributes and could not be resolved.

Emission always runs from the graph; the library stores no document source
text. For every graph `G`, `parse(emit(G))` equals `G`, and `emit` reaches a
fixed point after one round trip (SPEC.md §8).

## The format in brief

- **Four node kinds:** Object, Member, Leaf and Fallback.
- **Seven type sigils**, matched exactly: `[abc]` String, `[#]` Number,
  `[1.]` List, `[-]` Link or Ref, `[31]` Date, `[_]` Boolean, `[$]` Currency.
  Any other bracketed token at the start of a row (`[Card]`, `[Record]`) is a
  consumer tag and makes the row an Object.
- **One row grammar** for headings and list rows: an optional annotation, a
  head, then either `: rhs` (a member) or inline `name=value` properties,
  and an optional ` -> sink.path` last.
- **Indentation is the only thing that nests.** A sigil or tag changes a
  node's kind, never its depth.
- **Slots** (`{{ [abc] expr }}`) are opaque inline expressions; fenced code
  blocks attach to the nearest preceding row as payloads.
- **Streams:** `---` separates documents, and frontmatter at the start of a
  document is its envelope.
- **Literal blocks:** `- |` and `- name: |` take the item's indented lines
  verbatim, with no row grammar inside (SPEC.md amendment "literal blocks").

## Example

From SPEC.md §9, a schema-shaped node:

```markdown
# [Record] User

- [abc] id
- [abc] name
- [#] age
- [-] team -> teams.id
- active: Boolean
```

parses into one Object with tag `Record` and head `User`, holding five
Members, as printed by the `dump` example:

```text
document 0
  object [Record] User
    member [abc] id
    member [abc] name
    member [#] age
    member [-] team -> teams.id
    member active = Boolean
```

And indentation carrying depth: the three indented rows are siblings under
`[Card] Profile`. `[abc]` is a sigil, so `title` is a Member; `[Badge]` is
not, so `Status` is an Object.

```markdown
- [Card] Profile
  - [abc] title
  - [Badge] Status
  - visible: true
```

```text
document 0
  object [Card] Profile
    member [abc] title
    object [Badge] Status
    member visible = true
```

In code:

```rust
use libtypedmd::{emit, parse};

let stream = parse("# [Record] User\n\n- [abc] id\n- [#] age\n- active: Boolean\n");
let user = &stream.documents[0].nodes[0];
assert_eq!(user.as_object().unwrap().tag.as_deref(), Some("Record"));
assert_eq!(user.nodes.len(), 3);

// Emission runs from the graph, never from stored source.
assert_eq!(parse(&emit(&stream)), stream);
```

`parse_with(input, &ParseOptions::strict())` returns the first error instead
of falling back, and `ParseOptions::with_aliases` supplies a consumer's
alias table for sigil and tag spellings.

## Crates

- **`libtypedmd`**: the core. The lexical pre-pass (stream splitting and
  envelopes), a native block parser, the annotation grammar, the object
  graph, the sigil table, the emitter and the `MarkdownParser` trait that a
  block substrate implements. It has **no dependencies**. Use it on its own
  unless you need a second Markdown reader.
- **`libtypedmd_pulldown`**: `PulldownParser`, a block substrate that wraps
  `pulldown-cmark` (GFM disabled) behind the core's `MarkdownParser` trait.
  It slices every span from the source rather than rebuilding it from
  events. It exists to hold the native parser to the trait's
  interchangeability criterion; use it with `parse_with_substrate` when you
  want CommonMark's own block structure. Where the two readers can differ is
  listed in SPEC.md §6.2.

`libtypedmd::conformance` ships the corpus and comparison a substrate is
checked against: `conformance::check(&substrate)`.

Both crates share one version, set in the workspace manifest.

## Using it from Cargo

The crates are not on crates.io. Depend on them by git revision; this is the
0.2.0 release (tag `v0.2.0`):

```toml
[dependencies]
libtypedmd = { git = "https://github.com/tmzt/libtypedmd", rev = "a61c091e6420a1dfc40e89b8600cd26e628e5f7a" }
# Only if you want the pulldown-cmark substrate:
libtypedmd_pulldown = { git = "https://github.com/tmzt/libtypedmd", rev = "a61c091e6420a1dfc40e89b8600cd26e628e5f7a" }
```

## Building and testing

The repository is its own workspace root, so the core builds and tests
standalone:

```sh
cargo test
cargo test -p libtypedmd            # the core only
cargo test -p libtypedmd_pulldown   # the substrate and the conformance tests
```

`cargo test -p libtypedmd_pulldown` also writes
[REPORT.gen.md](REPORT.gen.md): every conformance-corpus document, followed
by what the emitter produces from the graph it parses into, with both
substrates' output shown where they disagree. It is generated, deterministic
and committed; it is never edited by hand.

Two examples:

```sh
# Print the object graph a file parses into (strict by default).
cargo run -p libtypedmd --example dump -- [--strict | --lenient] [FILE...]

# Read generated bodies with both substrates and report where they differ.
cargo run -p libtypedmd_pulldown --example sweep [-- --in-grammar]
```

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

## Architectural decisions

| Document | Date | Status | Decision |
|---|---|---|---|
| [Specification: libtypedmd](SPEC.md) | 2026-09-09 | Implemented | The format: four node kinds, seven sigils, one row grammar for headings and lists, indentation as the only nesting, lenient parsing with claims and an opt-in strict mode, emission from the graph with a round-trip guarantee; implemented in 0.1.0. |
| [Literal blocks — YAML-style `\|` rows whose indented text is taken verbatim, with no row grammar inside](docs/design/20261018_01_LITERAL_BLOCKS_PLAN.md) | 2026-10-18 | Implemented | `- \|` and `- name: \|` take the item's indented lines verbatim, so prose items may hold `: `; a new `LeafForm::Literal`; every step is done and it shipped as 0.2.0. |
