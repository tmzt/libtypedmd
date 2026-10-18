# VERSIONS

Each change of the crates' version and the public API changes it carries
(AGENTS.md, Rule 5). Internal changes are not listed, and there is no entry
without a version change. Entries are appended, oldest first, and never
edited.

## 0.1.0 — 2026-09-09 (`4982232`, tag `v0.1.0`)

Initial release: the core crate built from SPEC.md. Recorded retroactively
on 2026-10-18 from the commit; the tag was added on 2026-10-18.

## 0.2.0 — 2026-10-18 (`0544af1`, tag `v0.2.0`)

Literal blocks: YAML-style `- |` and `- name: |` rows
(`docs/design/20261018_01_LITERAL_BLOCKS_PLAN.md`, steps 1 to 4; SPEC.md
amendment "literal blocks"). Both crates share this version
(`[workspace.package]`). The commit named is the last one before the version
commit, which cannot name itself; the tag is on the version commit.

`libtypedmd`:

- Added `LeafForm::Literal`. **Breaking**: `LeafForm` is an exhaustive
  public enum, so a match without a wildcard no longer compiles.
- **Breaking**: `- |` rows now parse as a Leaf with `LeafForm::Literal`
  holding the item's indented lines verbatim, and `- name: |` rows (with or
  without a core sigil) as a Member whose RHS is one literal part holding the
  block; they used to be a leaf `|` or a member with RHS `|` followed by
  child paragraphs. Nothing inside a block is parsed. `- | text` is read as
  before.
- **Breaking**: the emitter writes a multi-line row leaf, a `Literal` leaf, and
  a member whose RHS is a multi-line (or lone `|`, or `->`-bearing, or empty)
  literal as `|` blocks; a multi-line row leaf used to emit as a paragraph,
  and a single-line row leaf `|` is now quoted.
- `conformance::CORPUS` gained three documents.

`libtypedmd_pulldown`: no API change; it conforms to the amended corpus.
