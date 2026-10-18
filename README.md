# libtypedmd

A generic object-graph format whose serialization is Markdown. The library
parses Markdown into a typed object graph and emits Markdown from that
graph; `- property: value` is YAML encoded as Markdown. The format is
specified in [SPEC.md](SPEC.md).

## Crates

- **`libtypedmd`** — the core: pre-pass, native parser, annotation
  grammar, object graph, sigil table and emitter. Zero dependencies.
- **`libtypedmd_pulldown`** — a second substrate wrapping `pulldown-cmark`
  behind the core's `MarkdownParser` trait, and the conformance report
  ([REPORT.gen.md](REPORT.gen.md)).

## Architectural decisions

| Document | Date | Status | Decision |
|---|---|---|---|
| [Specification: libtypedmd](SPEC.md) | 2026-09-09 | Implemented | The format: four node kinds, seven sigils, one row grammar for headings and lists, indentation as the only nesting, lenient parsing with claims and an opt-in strict mode, emission from the graph with a round-trip guarantee; implemented in 0.1.0. |
| [Literal blocks — YAML-style `\|` rows whose indented text is taken verbatim, with no row grammar inside](docs/design/20261018_01_LITERAL_BLOCKS_PLAN.md) | 2026-10-18 | In implementation | `- \|` and `- name: \|` take the item's indented lines verbatim, so prose items may hold `: `; a new `LeafForm::Literal`; ships as 0.2.0; approved by the owner and frozen while it is built. |
