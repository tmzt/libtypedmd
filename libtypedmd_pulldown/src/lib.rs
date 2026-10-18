//! A `pulldown-cmark` block substrate for `libtypedmd` (spec §6.2).
//!
//! # What this crate is for
//!
//! The [`MarkdownParser`] trait says two substrates conform iff they produce
//! identical block streams for every body. With one implementation that
//! sentence cannot fail. This is the second implementation, and the corpus in
//! [`libtypedmd::conformance`] is where it is held to the first.
//!
//! # The division of labour
//!
//! **pulldown-cmark decides the block structure. This adapter slices every
//! byte it reports out of the original body itself.**
//!
//! Nothing here is ever built from `Event::Text`. It could not be: pulldown
//! resolves entities (`&amp;` arrives as `&`), drops backslash escapes, drops
//! emphasis delimiters, and turns `[Card]` into a `Link` when a reference
//! definition is in scope. Annotation recognition runs on the row's verbatim
//! source, so a sigil must reach it byte for byte or a link definition
//! somewhere else in the file could quietly retype a row. Every [`Span`] this
//! adapter emits is `&body[..]`, located by an offset, not a reconstruction.
//!
//! # GFM is OFF
//!
//! [`Options::empty()`]: no tables, no footnotes, no strikethrough, and above
//! all **no task lists** — `- [x] done` is GFM's checkbox and `[_]` is this
//! format's boolean sigil. A substrate that turned `[x]` into a checkbox would
//! be eating annotations, and one that turned `| a | b |` into a table would
//! report a block shape the trait has no variant for.
//!
//! # Out-of-scope CommonMark
//!
//! [`Block`] is a closed five-variant enum and the graph is a closed
//! four-kind one: there is no block quote, no table, no HTML block, and no
//! catch-all in either. A construct outside the five shapes therefore
//! contributes **its verbatim source lines to the surrounding paragraph
//! run** — which is what the native substrate, having no rule for any of
//! them, already does. A paragraph becomes a `Leaf` and a leaf's text is its
//! VALUE, so nothing is lost: `> quoted` round-trips as `> quoted`, marker
//! included.
//!
//! Three inputs remain where the two substrates genuinely differ, all of them
//! cases where CommonMark's EXTENT for such a construct cannot be reproduced
//! by a line reader. They are listed in `SPEC.md` §6.2 and pinned in
//! `tests/conformance.rs`; both readings keep every source line.
//!
//! [`Options::empty()`]: pulldown_cmark::Options::empty

use std::ops::Range;

use libtypedmd::substrate::{is_thematic_break, list_marker, merge_adjacent_lists};
use libtypedmd::{Block, ListItem, MarkdownParser, Span};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

/// The `pulldown-cmark` substrate, GFM disabled.
#[derive(Debug, Clone, Copy, Default)]
pub struct PulldownParser;

impl PulldownParser {
    /// Exactly CommonMark. Every GFM extension is an annotation-eater or a
    /// block shape the trait cannot carry; see the module docs.
    ///
    /// This is public so the requirement can be PINNED rather than trusted.
    /// It has to be: because every span is sliced from the source, turning
    /// GFM on changes almost nothing this adapter reports — `[x]` stays in
    /// the row's text either way — so no output test can see the option. The
    /// requirement is real for a substrate that builds spans from events, and
    /// the only mechanical check available here is the value itself.
    pub const OPTIONS: Options = Options::empty();
}

impl MarkdownParser for PulldownParser {
    fn parse_body(&self, body: &str) -> Vec<Block> {
        let lines = Lines::new(body);
        let raw = read_events(body);
        let last = lines.count();
        Reader { lines: &lines }.sequence(&raw, 1, last, 0)
    }
}

// -- the event tree ----------------------------------------------------------

/// The block kinds this adapter distinguishes. Everything pulldown reports
/// that is not one of the five trait shapes lands in `Opaque`, whose lines go
/// into the surrounding text run; inline events never get here at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Heading(u8),
    Paragraph,
    Fence,
    List(bool),
    Item,
    Rule,
    Opaque,
}

#[derive(Debug)]
struct Raw {
    kind: Kind,
    range: Range<usize>,
    children: Vec<Raw>,
}

fn block_kind(tag: &Tag<'_>) -> Option<Kind> {
    Some(match tag {
        Tag::Heading { level, .. } => Kind::Heading(*level as u8),
        Tag::Paragraph => Kind::Paragraph,
        Tag::CodeBlock(CodeBlockKind::Fenced(_)) => Kind::Fence,
        Tag::List(ordered) => Kind::List(ordered.is_some()),
        Tag::Item => Kind::Item,
        // Indented code, block quotes, HTML blocks, tables, definition lists,
        // footnote definitions, metadata blocks — none of them is one of the
        // five shapes, and all of them read the same way: as their lines.
        Tag::CodeBlock(CodeBlockKind::Indented)
        | Tag::BlockQuote(_)
        | Tag::HtmlBlock
        | Tag::FootnoteDefinition(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::Table(_)
        | Tag::TableHead
        | Tag::TableRow
        | Tag::TableCell
        | Tag::MetadataBlock(_) => Kind::Opaque,
        // Inline: emphasis, strong, strikethrough, links, images, spans.
        _ => return None,
    })
}

/// Build the block tree. Inline `Start`/`End` pairs are skipped in step, which
/// is sound because an inline element can never contain a block.
fn read_events(body: &str) -> Vec<Raw> {
    let mut stack: Vec<Raw> = vec![Raw {
        kind: Kind::Opaque,
        range: 0..body.len(),
        children: Vec::new(),
    }];
    let mut inline_depth = 0usize;

    for (event, range) in Parser::new_ext(body, PulldownParser::OPTIONS).into_offset_iter() {
        match event {
            Event::Start(tag) => match block_kind(&tag) {
                Some(kind) if inline_depth == 0 => {
                    stack.push(Raw {
                        kind,
                        range,
                        children: Vec::new(),
                    });
                }
                _ => inline_depth += 1,
            },
            Event::End(_) => {
                if inline_depth > 0 {
                    inline_depth -= 1;
                } else if stack.len() > 1 {
                    let done = stack.pop().expect("checked");
                    stack
                        .last_mut()
                        .expect("the root never pops")
                        .children
                        .push(done);
                }
            }
            Event::Rule => {
                stack.last_mut().expect("root").children.push(Raw {
                    kind: Kind::Rule,
                    range,
                    children: Vec::new(),
                });
            }
            // Text, Code, Html, SoftBreak, HardBreak, FootnoteReference,
            // TaskListMarker: inline content. This adapter reads the source,
            // not the events.
            _ => {}
        }
    }

    while stack.len() > 1 {
        let done = stack.pop().expect("checked");
        stack.last_mut().expect("root").children.push(done);
    }
    stack.pop().expect("root").children
}

// -- lines -------------------------------------------------------------------

/// A body split the way the trait's spans are numbered: 1-based lines, 1-based
/// byte columns, `\r` stripped, trailing spaces gone.
struct Lines<'a> {
    starts: Vec<usize>,
    text: Vec<&'a str>,
}

impl<'a> Lines<'a> {
    fn new(body: &'a str) -> Self {
        let mut starts = Vec::new();
        let mut text = Vec::new();
        let mut offset = 0usize;
        for raw in body.split('\n') {
            starts.push(offset);
            text.push(raw.strip_suffix('\r').unwrap_or(raw));
            offset += raw.len() + 1;
        }
        Self { starts, text }
    }

    fn count(&self) -> usize {
        self.text.len()
    }

    fn text(&self, line: usize) -> &'a str {
        self.text[line - 1]
    }

    /// 1-based line containing a byte offset.
    fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index.max(1),
        }
    }

    /// The line a range STARTS its content on.
    ///
    /// A range may open at the newline that TERMINATES the line before it —
    /// pulldown opens a nested list there — and that byte is not content on
    /// the earlier line. Reading it as one makes the adapter re-read the
    /// parent's row and report it twice.
    fn start_line_of(&self, offset: usize) -> usize {
        let line = self.line_of(offset);
        if line < self.count()
            && offset.saturating_sub(self.starts[line - 1]) >= self.text(line).len()
        {
            line + 1
        } else {
            line
        }
    }

    /// The last line a range CARRIES CONTENT ON. `range.end` is exclusive and
    /// normally sits just past a newline, which belongs to the next line.
    ///
    /// A container's range can also stop inside the leading indentation of the
    /// line that broke it out — pulldown ends a list at the indent of the
    /// paragraph below it — and that line is not the list's. Reading it as the
    /// list's last line skips it, and the line is then lost, which is the one
    /// failure mode an adapter must not have.
    fn last_line_of(&self, range: &Range<usize>) -> usize {
        let line = self.line_of(range.end.saturating_sub(1).max(range.start));
        if line > 1 && range.start < self.starts[line - 1] {
            let text = self.text(line);
            let indent = text.len() - text.trim_start_matches(' ').len();
            // A BLANK line is all indentation and has no content to lose, and
            // inside a fence it is content in its own right. Only a line with
            // something after the indent can be stolen this way.
            if !text.trim().is_empty() && range.end <= self.starts[line - 1] + indent {
                return line - 1;
            }
        }
        line
    }

    /// A span starting at a byte offset within a line, trailing whitespace
    /// trimmed. This is `&body[..]`, never a rebuild.
    fn span_at(&self, line: usize, from: usize) -> Span {
        let text = self.text(line);
        let from = from.min(text.len());
        Span::new(text[from..].trim_end(), line, from + 1)
    }

    /// A whole line as a span, leading spaces dropped: what a paragraph line
    /// looks like to the trait.
    fn line_span(&self, line: usize) -> Span {
        let text = self.text(line);
        self.span_at(line, text.len() - text.trim_start_matches(' ').len())
    }
}

// -- conversion --------------------------------------------------------------

struct Reader<'a> {
    lines: &'a Lines<'a>,
}

impl Reader<'_> {
    /// Convert one container's children, covering `first..=last`.
    ///
    /// The unit this walks is not a block but a TEXT RUN: consecutive lines
    /// that carry no structure, ended by a blank line, a thematic break, or a
    /// block of one of the five shapes. Three kinds of line feed the run —
    /// a paragraph's, an out-of-scope construct's, and a line no event claimed
    /// at all — and that is the whole out-of-scope rule:
    ///
    /// * `> quoted` after a paragraph line is one paragraph, not two. The
    ///   native reader has no rule for `>` so it never breaks there, and a
    ///   substrate that broke would report a boundary the format cannot mean.
    /// * a link REFERENCE DEFINITION is reported by pulldown as no events
    ///   whatsoever. Reading the gaps from the source is what keeps the line;
    ///   building only from events would silently delete it.
    /// * in a TIGHT list `- foo\n  bar` there is no paragraph event either,
    ///   only inline ones, and the continuation line arrives the same way.
    ///
    /// `floor` suppresses lines at or before it — an item's own row, which the
    /// item reports separately.
    fn sequence(&self, children: &[Raw], first: usize, last: usize, floor: usize) -> Vec<Block> {
        let mut out: Vec<Block> = Vec::new();
        let mut run: Vec<Span> = Vec::new();
        let mut next = 0usize;
        let mut line = first.max(floor + 1);

        while line <= last {
            if let Some(child) = children.get(next) {
                if self.lines.start_line_of(child.range.start) <= line {
                    let end = self.lines.last_line_of(&child.range).max(line);
                    if self.is_text(child) {
                        self.text_lines(&mut out, &mut run, line, end);
                    } else {
                        flush(&mut out, &mut run);
                        out.extend(self.block(child));
                    }
                    line = end + 1;
                    next += 1;
                    continue;
                }
            }
            self.text_lines(&mut out, &mut run, line, line);
            line += 1;
        }
        // Children beyond `last` cannot normally exist, but a range that
        // disagreed with the line arithmetic must not silently drop source.
        // The floor still applies: a child that lies entirely on the item's
        // own row IS that row, and the item reported it already.
        for child in children.iter().skip(next) {
            let start = self.lines.start_line_of(child.range.start).max(floor + 1);
            let end = self.lines.last_line_of(&child.range);
            if start > end {
                continue;
            }
            if self.is_text(child) {
                self.text_lines(&mut out, &mut run, start, end);
            } else {
                flush(&mut out, &mut run);
                out.extend(self.block(child));
            }
        }
        flush(&mut out, &mut run);
        merge_adjacent_lists(out)
    }

    /// Does this block contribute LINES to the surrounding run rather than a
    /// block of its own? Paragraphs do; so does every construct outside the
    /// five shapes; so does a SETEXT heading, which this format does not have
    /// — `---` is the stream separator and the emitter only ever writes ATX,
    /// so a setext underline is read as the line it is.
    fn is_text(&self, raw: &Raw) -> bool {
        match raw.kind {
            Kind::Paragraph | Kind::Opaque => true,
            Kind::Heading(_) => {
                self.lines.start_line_of(raw.range.start) != self.lines.last_line_of(&raw.range)
            }
            _ => false,
        }
    }

    /// Feed `first..=last` into the run as verbatim source lines. A blank line
    /// ends the run; a thematic break ends it and carries no node.
    fn text_lines(&self, out: &mut Vec<Block>, run: &mut Vec<Span>, first: usize, last: usize) {
        for line in first..=last {
            let text = self.lines.text(line);
            if is_thematic_break(text) {
                flush(out, run);
                out.push(Block::ThematicBreak);
            } else if text.trim().is_empty() {
                flush(out, run);
            } else {
                run.push(self.lines.line_span(line));
            }
        }
    }

    /// A block of one of the five shapes. Text-carrying kinds never reach
    /// here; [`Reader::sequence`] routes them into the run.
    fn block(&self, raw: &Raw) -> Vec<Block> {
        let first = self.lines.start_line_of(raw.range.start);
        let last = self.lines.last_line_of(&raw.range);
        match raw.kind {
            Kind::Rule => vec![Block::ThematicBreak],
            Kind::Heading(level) => {
                vec![Block::Heading {
                    level,
                    source: self.heading_span(first, level),
                }]
            }
            Kind::Fence => vec![self.fence(first, last)],
            Kind::List(ordered) => {
                let items = raw.children.iter().map(|item| self.item(item)).collect();
                vec![Block::List { ordered, items }]
            }
            // An item outside a list cannot happen; treat it as one anyway
            // rather than dropping source.
            Kind::Item => vec![Block::List {
                ordered: false,
                items: vec![self.item(raw)],
            }],
            Kind::Paragraph | Kind::Opaque => {
                let mut out = Vec::new();
                let mut run = Vec::new();
                self.text_lines(&mut out, &mut run, first, last);
                flush(&mut out, &mut run);
                out
            }
        }
    }

    /// The heading's inline source: past the indent, the `#` run and the
    /// spaces after it, with an ATX closing sequence removed.
    fn heading_span(&self, line: usize, level: u8) -> Span {
        let text = self.lines.text(line);
        let indent = text.len() - text.trim_start_matches(' ').len();
        let rest = &text[indent..];
        let hashes = rest
            .bytes()
            .take_while(|b| *b == b'#')
            .count()
            .min(level as usize);
        let after = &rest[hashes..];
        let spaces = after.len() - after.trim_start_matches(' ').len();
        let from = indent + hashes + spaces;
        let mut span = self.lines.span_at(line, from);
        let stripped = span.text.trim_end_matches('#');
        if stripped.len() != span.text.len() && (stripped.is_empty() || stripped.ends_with(' ')) {
            span.text = stripped.trim_end().to_string();
        }
        span
    }

    /// The fence's info string and literal body, sliced from the source. The
    /// body of a fence is not inline content and pulldown's `Text` events for
    /// it are decoded; these lines are not.
    fn fence(&self, first: usize, last: usize) -> Block {
        let open = self.lines.text(first);
        let indent = open.len() - open.trim_start_matches(' ').len();
        let rest = &open[indent..];
        let ch = rest.chars().next().unwrap_or('`');
        let run = rest.chars().take_while(|c| *c == ch).count();
        let info = open[indent + run..].trim().to_string();

        let mut body_lines: Vec<&str> = Vec::new();
        for line in first + 1..=last {
            let text = self.lines.text(line);
            let closing_indent = text.len() - text.trim_start_matches(' ').len();
            let closing = &text[closing_indent..];
            if closing_indent <= 3
                && closing.starts_with(ch)
                && closing.chars().take_while(|c| *c == ch).count() >= run
                && closing.trim_end_matches(ch).trim().is_empty()
            {
                break;
            }
            let strip = (text.len() - text.trim_start_matches(' ').len()).min(indent);
            body_lines.push(&text[strip..]);
        }
        let body = if body_lines.is_empty() {
            String::new()
        } else {
            let mut joined = body_lines.join("\n");
            joined.push('\n');
            joined
        };
        Block::CodeFence {
            info,
            body,
            line: first,
        }
    }

    fn item(&self, raw: &Raw) -> ListItem {
        let row = self.lines.start_line_of(raw.range.start);
        let last = self.lines.last_line_of(&raw.range);
        let text = self.lines.text(row);
        let content = list_marker(text).map(|(_, content)| content).unwrap_or(0);
        ListItem {
            source: self.lines.span_at(row, content),
            blocks: self.sequence(&raw.children, row, last, row),
        }
    }
}

fn flush(out: &mut Vec<Block>, pending: &mut Vec<Span>) {
    if !pending.is_empty() {
        out.push(Block::Paragraph {
            lines: std::mem::take(pending),
        });
    }
}
