# Specification: `libtypedmd`

**Concept:** A generic object-graph format whose serialization is Markdown. The library parses Markdown into a typed object graph and emits Markdown from that graph. The core is strictly agnostic to consuming domains; everything a consumer interprets is listed in Part II.

**The mental model:** `- property: value` is YAML encoded as Markdown. A Markdown list becomes a mapping; indentation is nesting, exactly as in YAML. One key, one right-hand side. Whether an RHS is a *type* or a *value* is the consumer's reading, never the core's.

---

# Part I — Generic Core

## 1. Data Model

* **Stream:** The file. An ordered sequence of documents. A file with no structural `---` is one document with an empty envelope.
* **Document:** A `---`-delimited unit: an **Envelope** (frontmatter) plus a tree of nodes under an implicit root.
* **Node kinds:** exactly four.
  * **Object:** tag (optional bracketed token, verbatim), head (opaque text, may contain slots), an ordered node list (members and children in one sequence), payloads (ordered fenced blocks), optional sink, carrier flag (see §8).
  * **Member:** name, optional core sigil, optional RHS (opaque text, may contain slots), optional sink, ordered child nodes (nested property sets), placement flag (see §8).
  * **Leaf:** verbatim text. A plain list row or paragraph. Its text is its *value*, not a parse cache.
  * **Fallback:** verbatim span plus a claim record (§7).
* **Slot:** a `{{ }}` inline expression — opaque text with an optional leading type sigil.
* **Sink:** a `-> target.path` suffix. Resolution is entirely the consumer's.

### 1.1 The Two Property Planes

Structurally distinct; never merged.

* **Header properties (declaration plane):** tag, head, inline `attr=value` pairs. They declare the object and travel with every restatement of it.
* **Envelope (emission plane):** facts about this *emission* of the document — base revision, provenance. Flat `key: value` lines, values opaque, no nesting.

### 1.2 One Member, Two Spellings

`- name: value` (a row) and `name=value` (inline on the parent's row) produce **the same node**: a Member with one name and one RHS. The parser records which spelling was seen in the placement flag; placement is presentation only and consumers must ignore it. Inline names must be bare identifiers (`[A-Za-z_][A-Za-z0-9_-]*`); row-member names may contain spaces.

## 2. Sigils and Tags

The core recognizes exactly **seven canonical ASCII sigils**, by exact token match — no patterns:

| Sigil | Type |
| --- | --- |
| `[abc]` | String |
| `[#]` | Number |
| `[1.]` | List / Collection |
| `[-]` | Link / Ref |
| `[31]` | Date |
| `[_]` | Boolean |
| `[$]` | Currency |

**A sigil is a type.** It appears in exactly two positions and prefixes what it types in both:

1. Leading a row: `- [abc] id` — member `id` of type String.
2. Leading a slot: `{{ [abc] svc.name }}` — types the expression.

A sigil is never a value, never a standalone token, never a tag.

**Tag vs sigil:** membership in the table above is the whole test. A row-leading bracketed token in the core table produces a typed Member; any other bracketed token (`[123]`, `[Zorp]`, `[Screen]`) is a consumer token, carried verbatim, and produces an Object with that tag. The core emits canonical ASCII sigils only; legacy spellings (`⚯`, `[o]`, …) resolve through a consumer-supplied alias table at parse time (§11).

## 3. Row Grammar

The same production governs heading rows and list rows; only the marker differs.

```
row         := indent marker SP annot? head? tail
marker      := "-"                (list row)
             | "#"{1..6}          (heading row)
annot       := "[" token "]"      (core table → sigil; otherwise → tag)
tail        := colon-rhs sink?    (member row)
             | inline-prop* sink? (object row)
colon-rhs   := ":" SP rest-of-line
inline-prop := SP name "=" value
sink        := SP "->" SP path
```

Rules, in order:

1. At most one annotation, recognized only as the first token after the marker. Later bracketed tokens are ordinary text, except inside slots.
2. Row classification: core sigil → Member. Non-core tag → Object. No annotation but a colon-RHS → Member. No annotation, no colon → Leaf (list row) or Object head restatement (heading row: an untagged heading is an Object with empty tag).
3. Ordered lists (`1.` markers) are accepted as list rows; the ordinal is not preserved.
4. `value` is a bare token (no whitespace, `"`, `=`, or `{{`), a double-quoted string (escapes `\"` and `\\`), or a slot.
5. The sink, when present, is last on the row.

### 3.1 Head Delimitation

A head may contain spaces. It ends at the first of:

1. an unquoted bare identifier immediately followed by `=` (start of inline properties);
2. an unquoted ` -> ` (space, arrow, space);
3. on a tagless row, an unquoted `:` followed by whitespace or end of line — a colon not followed by whitespace (`http://…`) does not end it;
4. end of line.

A head beginning with `"` runs to the closing quote and may then contain any of the above literally.

### 3.2 Slots

* Permitted in: heads, inline property values, member RHS. Not recognized inside fenced payloads or envelope values.
* Must open and close on one line; never nest. The optional sigil must be the first token inside the braces; the rest is opaque expression text.
* `\{{` is a literal `{{`. An unclosed `{{` is literal text in lenient mode and an error in strict mode.

## 4. Nesting

**INDENTATION IS THE ONLY THING THAT NESTS.** Every indented row is a child node. A sigil or tag never changes a node's depth. This is the single most important invariant in the format.

1. **Heading rows:** a heading's parent is the nearest open heading Object of *lower* level (the document root if none). Heading levels order the hierarchy; they do not set absolute depth, so skipped levels (`###` directly under `#`) create no phantom nodes. A heading closes all open list nesting and all open headings of equal or higher level.
2. **List rows:** a top-level list attaches to the innermost open heading Object (the root if none). Each level of list nesting, as reported by the substrate, is one level deeper.
3. **Composition:** depth of a list row = depth of its owning heading Object + its list nesting.
4. Members, sub-Objects, and Leaves at the same indentation are **siblings in one ordered sequence** under the same parent.

## 5. Payloads and Duplicates

* **Payload attachment:** a fenced code block attaches to the node of the nearest preceding row at the fence's block position — a fence nested inside a list item attaches to that item's node; a fence at heading level attaches to the innermost open heading Object. A node may carry multiple payloads, in order; each keeps its info string verbatim. Payload bodies are literal: no slot or annotation recognition.
* **Duplicate keys:** within one plane on one node (envelope keys, or member names under one parent), the **last occurrence wins** and earlier occurrences are dropped from the graph. Strict mode rejects the duplicate at the second occurrence's location.

## 6. Parsing Architecture

### 6.1 Lexical Pre-Pass (owned by libtypedmd)

Stream splitting and envelope peeling run *before* the Markdown substrate sees any text.

1. A standalone `---` at file start or after a blank line is structural.
2. A structural `---` at document start opens frontmatter iff a closing `---` arrives before any content.
3. A structural `---` after content closes the document and starts the next.
4. The substrate parses only document *bodies*. A thematic break inside a body must be spelled `***` or `___`.

### 6.2 The `MarkdownParser` Trait

The substrate supplies block structure and **verbatim inline spans**; it never interprets inlines. A conforming substrate reports, in document order, with nesting:

* **Heading** — level 1–6, verbatim inline source of the heading text.
* **List / ListItem** — bullet vs ordered, nesting depth, verbatim first-line source, nested child blocks.
* **Paragraph** — verbatim source lines.
* **CodeFence** — info string, literal body.
* **ThematicBreak** — `***` / `___` only (the pre-pass owns `---`).

Every verbatim span is a byte-exact slice of the body with a source location. No emphasis, link, HTML, or GFM interpretation. **Interchangeability criterion:** two substrates conform iff they produce identical block streams for every body.

The criterion is executable: `libtypedmd::conformance` ships the corpus and the comparison, and `conformance::check(&substrate)` is what a substrate author runs. `libtypedmd_pulldown` is the second implementation it is measured against.

**Execution order:** annotations (sigils, tags, inline properties, sinks, slots) are recognized *after* the block parse, on the row's verbatim source, *before* any inline Markdown interpretation — so a sigil can never be consumed as a CommonMark link.

**Out of scope.** There are five block shapes and no catch-all, and the graph has four node kinds and no catch-all either. Every other CommonMark construct — **block quote, HTML block, setext heading, indented code, link reference definition, and every GFM construct** — is out of scope, and a substrate reports it as **its verbatim source lines, contributed to the surrounding paragraph run**. Not dropped, not unwrapped, not given a variant: `> quoted` is a Paragraph line whose text is `> quoted`, marker included. A Paragraph becomes a Leaf and a leaf's text is its VALUE, so the content round-trips unchanged.

Two rules make that reading agree across substrates. A **thematic break** ends a run and carries no node, wherever it appears. A **`>` line** ends a list item's lazy continuation — it is the one out-of-scope construct that always interrupts, so both readers can agree on it cheaply.

**The boundary.** Agreement is over the format's grammar, not over all of CommonMark. Where a reader's *extent* for an out-of-scope construct cannot be reproduced by a line-by-line reader, the substrates may differ, and the difference is pinned in `libtypedmd_pulldown/tests/conformance.rs` rather than left to be discovered:

1. An **HTML block** runs to a blank line, swallowing lines a line reader takes as blocks of their own (`<div>` then `## Heading`).
2. An **ordered list whose first number is not 1** cannot interrupt a paragraph in CommonMark; a line reader starts a list at any marker.
3. A **tab** is four columns of indentation in CommonMark and none at all to a reader that counts spaces.

In all three, both readings keep every source line; only the shape differs. Measured over 37,059 generated bodies, these are the only three.

**List grouping is not reported.** `List` carries `ordered` and `items` and nothing else — no bullet, no delimiter, no start ordinal (§3 rule 3) — so where one list ended and the next began is not expressible, and the assembler cannot see it: the items of two adjacent lists become siblings in exactly the sequence the items of one list would. Readers disagree about it constantly, so adjacent lists of the same orderedness are merged (`substrate::merge_adjacent_lists`) before anything compares them.

**Rejected: a passthrough `Block` variant.** The alternative was to grow `Block` a verbatim/passthrough variant so a substrate could report a block quote as itself. It was rejected because it buys nothing and costs the closed enum: `NodeKind` has no block-quote kind either, so the assembler would have to lower the new variant to a Leaf carrying the same text this rule already produces — the identical graph, reached through a wider trait. Making it carry more than a Leaf means a block-quote node kind, an emitter spelling, and a round-trip guarantee for a construct the format never emits. The variant would also be a place to put things: every future CommonMark shape becomes a passthrough rather than a decision.

## 7. Leniency, Claims, and Strict Mode

**Leniency is the default.** The producer is an LLM and will not always be compliant; this is also why the annotation grammar is kept small. Lenient parsing never fails on content: anything unrecognized becomes a Fallback node carrying its verbatim span as its value plus a **claim record**.

**Claim record shape:**

| Field | Contents |
| --- | --- |
| `shape` | `Sigil` (row led with a bracketed token), `Attrs` (row contained `name=` pairs), or `None` |
| `token` | the bracketed token, verbatim, if any |
| `attrs` | the `(name, value)` pairs as scanned, in order |
| `location` | document index, 1-based line, 1-based byte column |

**Strict mode** is an opt-in parser option: invalid annotations are rejected anywhere. The predicate: the parse fails iff a node fell back with `shape ≠ None` — it claimed to describe an object (offered a sigil or attributes) and could not be resolved. Unannotated prose claims nothing and is preserved in both modes. A well-formed document parses identically under both modes.

**Error model:** lenient parse returns a graph unconditionally (only invalid UTF-8 fails). Strict parse returns either the graph or the **first** error: message, location (document index, line, byte column), and the offending claim record.

## 8. Emission and Round-Trip

The library stores **no document- or stream-level source text**; the graph's types have no field that could hold it, so emission is structurally unable to return stored input. Emission always runs from the graph.

**Guarantee (testable):**

1. For every graph `G`: `parse(emit(G)) ≡ G` — graph equality over every field in §1, including order, ignoring locations.
2. For every graph `G`: `emit(parse(emit(G))) == emit(G)`, byte for byte (emitter fixed-point).
3. `emit(parse(input))` need **not** equal `input`. Byte equality of arbitrary input is explicitly not the guarantee.

**Canonical form:** deterministic, so the fixed-point is checkable.

1. Bullet `-`, one space after the marker; two-space indent per list level; one blank line between top-level blocks, none between list rows.
2. Row order: marker, annotation, one space, head, then colon-RHS (`: ` + RHS) or inline properties, sink (` -> path`) last.
3. Properties and children emit in graph order. Sigils emit as canonical ASCII only.
4. Values: bare if free of whitespace, `"`, `=`, and `{{`; otherwise double-quoted with `\"` and `\\`. Slots emit verbatim with single interior padding spaces: `{{ [abc] expr }}`.
5. **Placement:** a Member with `placement = Inline` emits on its parent's row iff its name is a bare identifier, it has no sigil, sink, or children, and no non-inline sibling precedes it in graph order; otherwise it demotes to a row. Nodes built programmatically default to row.
6. **Carrier:** an Object with `carrier = Heading` emits as an ATX heading at level 1 + its count of heading-carrier ancestors (cap 6; beyond the cap, or when a row-carrier sibling follows it, it demotes to a list row). Nodes built programmatically default to row. Carrier, like placement, is presentation only.
7. Envelope: emitted as `---` / `key: value` lines in graph order / `---`, only when non-empty. Documents joined by a single `---` line.

## 9. Examples

**Schema-shaped node** — children are all members; no sub-objects. `active: Boolean` shows an RHS a schema consumer reads as a type:

```markdown
# [Record] User

- [abc] id
- [abc] name
- [#] age
- [-] team -> teams.id
- active: Boolean
```

**Widget-shaped node** — properties inline on the object's own row; children are sub-objects; slots in a property value and in heads:

```markdown
# [Screen] Inbox

- [List] source={{ [#] inbox.threads }} item_alias="thread"
  - [Text] {{ [abc] thread.subject }}
  - [Text] {{ [abc] thread.sender }}
```

**Indentation carries depth** — the three indented rows are siblings at one depth under `[Card] Profile`: a typed member, a sub-object, a member. Rewriting `- [abc] title` to `- [Badge] title` changes the node's *kind* (Member to Object), never its depth:

```markdown
- [Card] Profile
  - [abc] title
  - [Badge] Status
  - visible: true
```

## 10. Workspace and Crates

* **Location:** bare repository at `./repos/libtypedmd`, soft-cloned to `./deps/libtypedmd`.
* **`libtypedmd` (core):** pre-pass, native parser, annotation grammar, object graph, sigil table, emitter, `MarkdownParser` trait. **Zero dependencies.**
* **`libtypedmd_pulldown` (adapter):** sibling crate wrapping `pulldown-cmark` with GFM disabled, behind the trait.
* **Agnosticism gate:** the core crate forbids consumer terminology (`tsx`, `screen`, `highbay`, `storyboard`, …).

---

# Part II — Consumer Contract

The core records structure; a consumer supplies meaning. A consumer provides, at parse time or above:

1. **Alias table:** maps consumer spellings onto canonical tokens, for **both sigils and tags**. The core applies it during annotation recognition; without one, only the seven canonical sigils match and every other bracketed token is carried verbatim. An alias never changes a node's kind: a sigil alias resolves to a core sigil (Member), a tag alias resolves to a tag (Object).
2. **Tag vocabulary:** interpretation of every non-core bracketed token (`[Screen]`, `[List]`, …). The core carries them verbatim.
3. **RHS reading:** whether a member's RHS is a type (schema reading) or a value (widget reading), per context. The core records one opaque RHS.
4. **Resolution:** sink paths (`-> teams.id`) and slot expression text. Scope rules are wholly consumer-defined.
5. **Identity and restatement:** the core never merges documents. **Every `---`-delimited document in a stream is STORED as its own tree, in file order, including when two documents restate the same object.** The core reports the restatement as two trees and never reconciles them; recognizing the identity and merging is consumer-level.
6. **Payload interpretation:** info strings on fenced blocks (which is a label, which is code) are consumer semantics.

**Highbay:** `highbay_prosemd` routes its lowerings through the `MarkdownParser` trait and retains only Highbay schema interpretation and its legacy alias tables. No Highbay term crosses into the core.

---

# Open

*(none — the two items previously listed here are settled in Part II: the alias
table covers tags as well as sigils, and a stream stores restatements as
separate documents rather than merging them.)*
