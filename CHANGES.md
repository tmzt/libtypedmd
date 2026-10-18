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
