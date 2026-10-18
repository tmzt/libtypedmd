---
title: Literal blocks — YAML-style `|` rows whose indented text is taken verbatim, with no row grammar inside
date: 2026-10-18
status: In implementation
covers:
  - "`- |` rows: the item's indented lines as one verbatim Leaf"
  - "`- name: |` rows: the same block as a Member's RHS"
  - Emission of multi-line leaves and RHS values as literal blocks
  - The SPEC.md amendment that adds them, and version 0.2.0
sources:
  - The owner's instruction, 2026-10-18 (a consumer's dialogue file needs
    list items that may hold `: ` without becoming members) "I was thinking
    yaml style | strings would disable the : rule"
  - SPEC.md §3 (row grammar, rule 2), §7, §8
related:
  - SPEC.md (amended by step 1)
---

# Literal blocks

## Why

SPEC.md §3 rule 2 reads a list row with no annotation and a colon-RHS as
a Member. A list of prose items (wordings, notes, quotations) can't hold
`: ` in an item without turning it into a member whose name is the text
before the colon. Today the only way round it is a row that is entirely
one double-quoted string (the rule the emitter uses), which is noisy to
write by hand and can't span lines.

YAML solves the same problem with literal block scalars: `- |` or
`key: |`, then indented lines taken as text. The format already says
`- property: value` is YAML encoded as Markdown, so this borrows the
YAML spelling.

Today `- |` parses as a Leaf `"|"` with the indented text as a child
Paragraph Leaf, and `- say: |` as a Member with RHS `"|"` and the same
child. This plan gives those rows their YAML meaning.

## The rule

1. **Leaf block.** A list row whose whole content after the marker
   (trimmed) is `|` is a Leaf with `form: LeafForm::Literal`. Its text is
   the item's continuation: every source line after the row up to the end
   of the list item, as the substrate's item extent gives it, with the
   least common indent of the non-blank lines removed, trailing blank
   lines dropped and newlines kept (`\n`). The text is sliced from the
   document body by location, not rebuilt from the substrate's blocks, so
   a line starting `- `, `#`, `>` or ``` ``` ``` inside the block stays
   text and creates no node.
2. **Member block.** A row `- name: |` (a member row whose RHS, trimmed,
   is exactly `|`), with or without a core sigil, has as its RHS one
   `TextPart::Literal` holding the block, taken the same way. Slots are
   not recognised inside it.
3. **Nothing nests in a block.** The node has no children from the
   block's lines; payloads and fences inside it are text.
4. **Empty block.** `- |` with no indented lines is an empty literal Leaf;
   `- name: |` with none is a Member with an empty literal RHS.
5. **Strict mode.** A literal block claims nothing (claim shape `None`);
   it never fails strict parsing.
6. **Inline form.** `- | text` (text on the same row) is not a literal
   block: it is read as today. YAML does not allow it either.

## Emission (SPEC.md §8)

1. A Leaf with `form: Literal`, or any Leaf whose text contains `\n`,
   emits as `- |` followed by its lines, each indented two spaces deeper
   than the row's marker (blank lines emitted empty).
2. A Member whose RHS is one literal part containing `\n` emits as
   `name: |` followed by its lines, the same way. A sigil, if any, keeps
   its place: `- [abc] name: |`.
3. A single-line Leaf keeps today's rule (bare, or one double-quoted
   string when it holds a colon, a leading `[`, a leading `"` or edge
   whitespace); a single-line Leaf with `form: Literal` emits as a block.
4. Both guarantees hold: `parse(emit(G)) ≡ G` and the emitter's fixed
   point.

## Interface changes

- `LeafForm::Literal` added (an exhaustive public enum: **Breaking**).
- `Leaf.text` for a literal block is the block's text as above.
- No change to `ParseOptions`, the `MarkdownParser` trait or `Block`: the
  substrate already reports each list item's extent and locations.

## Steps

### Step 1 — the SPEC.md amendment

An amendment appended to SPEC.md adding §3 rule 6 (literal blocks, "The
rule" above) and §8 canonical form rule 8 ("Emission" above), and the
`Literal` leaf form to §1.

**Done when:** the amendment is committed with this plan's status set to
`In implementation`.

### Step 2 — parsing

The assembler recognises both row shapes after the block parse and
before annotation recognition, slices the block from the body, and drops
the substrate's child blocks for that item. Both substrates (the native
reader and `libtypedmd_pulldown`) give the same graph; the conformance
corpus gains the cases below.

Tests (core): a block holding `a: b`, `x=y`, `- item`, `# heading`,
`> quote`, a fence, `{{ slot }}` and a blank line between two paragraphs;
`- name: |` and `- [abc] name: |`; an empty block; a block nested two
list levels deep; `- | text` read as today; strict mode accepts all of
them.

**Done when:** the tests pass on both substrates and `REPORT.gen.md` shows
the new corpus entries.

### Step 3 — emission

The emitter rules above.

Tests: every parsing test's graph emits and parses back equal, and emits
the same bytes twice; a programmatically built Leaf with `"a: b\nc"`
round-trips.

**Done when:** the round-trip tests pass.

### Step 4 — version 0.2.0

The two crates' versions move to `[workspace.package] version = "0.2.0"`,
with the VERSIONS.md entry (`LeafForm::Literal`, **Breaking**; the `|`
rows' new reading, **Breaking**), tagged `v0.2.0`.

**Done when:** `cargo test` passes in the workspace and `v0.2.0` is
tagged.

## Amendment — 2026-10-18 15:00 EDT: implementation started

Implementation of this plan has started with step 1 (the SPEC.md
amendment). The status is now `In implementation` and this document is
immutable (AGENTS.md, Rule 1.5); it changes only on the owner's explicit
instruction.
