# CHANGES

Planned work, plans and architectural changes, appended oldest first
(AGENTS.md, Rule 0).

## 2026-10-18 13:49 EDT — The repository rules

On the owner's instruction, the repository adopts libcontrols' critical
rules 0 to 5 (AGENTS.md), with SPEC.md kept at the root as the format's
specification and changed only by amendments. README.md, CHANGES.md and
VERSIONS.md are added; 0.1.0 is recorded retroactively and tagged.

## 2026-10-18 13:49 EDT — Literal blocks

Plan: docs/design/20261018_01_LITERAL_BLOCKS_PLAN.md (Draft).

- YAML-style `- |` and `- name: |` rows whose indented text is taken
  verbatim, a new `LeafForm::Literal`, emission of multi-line text as
  blocks, and version 0.2.0. Not started.

## 2026-10-18 15:04 EDT — Literal blocks: departures from the plan as built

Plan: docs/design/20261018_01_LITERAL_BLOCKS_PLAN.md (In implementation).
Small departures needed to make it work (Rule 1.7); the plan is not edited.

- Step 3, emission rule 1: "any Leaf whose text contains `\n`" emits as a
  block applies to Row and Literal leaves. A Paragraph-form leaf keeps
  emitting as a paragraph, because emitting it as `- |` would read back with
  a different `form` and break `parse(emit(G)) == G`. The test that pinned
  the old demotion of a multi-line row leaf to a paragraph now expects a
  literal block.
- Step 3, additions the round-trip guarantee needs: a single-line row leaf
  whose text is exactly `|` is quoted (so it does not open a block); a member
  whose RHS is a literal `|`, contains `->` (it would read back as a sink)
  or is present and empty emits as a `name: |` block.
- Step 2 and 3, ordering: the new conformance-corpus entries are added with
  the emitter (step 3's commit), because the corpus is also round-tripped
  through the emitter. Step 2's agreement between the two substrates is
  tested meanwhile by `libtypedmd_pulldown/tests/literal.rs`.
- Known limits of the format, not fixed here: a block cannot carry leading
  indentation common to all its lines, trailing blank lines, or trailing
  spaces (parsing drops them, as the plan says), and a literal node's
  children, payloads or sink have no spelling after a block and are not
  emitted.
- Tooling: neither crate passed `cargo fmt --check` at 0.1.0, so each got a
  formatting-only commit before its first code change (Rule 3.4).
