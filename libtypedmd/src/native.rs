//! The native block substrate: a small, dependency-free Markdown block
//! reader implementing [`MarkdownParser`].
//!
//! It reads exactly the five block shapes the trait names and NOTHING else:
//! no emphasis, no links, no HTML, no GFM, no setext headings, no indented
//! code blocks. Inline text is handed on as a byte-exact span.

use crate::substrate::{
    is_thematic_break, list_marker, merge_adjacent_lists, Block, ListItem, MarkdownParser, Span,
};

/// The dependency-free substrate. Interchangeable with any other conforming
/// implementation (spec §6.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeParser;

impl MarkdownParser for NativeParser {
    fn parse_body(&self, body: &str) -> Vec<Block> {
        let mut lines: Vec<Line> = body
            .split('\n')
            .enumerate()
            .map(|(idx, raw)| Line {
                text: raw.strip_suffix('\r').unwrap_or(raw).to_string(),
                line: idx + 1,
                col_offset: 0,
            })
            .collect();
        // A body ending in a newline splits to a final EMPTY element, which is
        // the terminator and not a line of the document. It has to go: an
        // UNTERMINATED fence reads to end of input, so the phantom would land
        // in its payload as a blank line that is not in the source.
        if lines.len() > 1 && lines.last().map(|l| l.text.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        merge_adjacent_lists(parse_blocks(&lines))
    }
}

/// A body line, possibly already de-indented by `col_offset` bytes so that a
/// nested block can be parsed as if it were at the left margin. `col_offset`
/// is what puts the columns back.
#[derive(Debug, Clone)]
struct Line {
    text: String,
    line: usize,
    col_offset: usize,
}

impl Line {
    fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    fn indent(&self) -> usize {
        self.text.len() - self.text.trim_start_matches(' ').len()
    }

    /// A span covering the line from byte `from` to the end, trailing
    /// whitespace trimmed.
    fn span_from(&self, from: usize) -> Span {
        let from = from.min(self.text.len());
        let text = self.text[from..].trim_end();
        Span::new(text, self.line, self.col_offset + from + 1)
    }

    /// The same line with `n` leading bytes removed and the offset kept.
    fn dedent(&self, n: usize) -> Line {
        let n = n.min(self.text.len());
        Line {
            text: self.text[n..].to_string(),
            line: self.line,
            col_offset: self.col_offset + n,
        }
    }
}

fn parse_blocks(lines: &[Line]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = &lines[i];
        if line.is_blank() {
            i += 1;
            continue;
        }
        if let Some(block) = read_fence(lines, &mut i) {
            out.push(block);
            continue;
        }
        if is_thematic_break(&line.text) {
            out.push(Block::ThematicBreak);
            i += 1;
            continue;
        }
        if let Some((level, from)) = heading_at(&line.text) {
            out.push(Block::Heading {
                level,
                source: heading_span(line, from),
            });
            i += 1;
            continue;
        }
        if list_marker(&line.text).is_some() {
            out.push(read_list(lines, &mut i));
            continue;
        }
        out.push(read_paragraph(lines, &mut i));
    }
    out
}

// -- headings ----------------------------------------------------------------

/// `#{1,6}` followed by a space or end of line. Returns the level and the
/// byte offset where the heading text starts.
fn heading_at(text: &str) -> Option<(u8, usize)> {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &text[indent..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    let spaces = after.len() - after.trim_start_matches(' ').len();
    Some((hashes as u8, indent + hashes + spaces))
}

/// The heading's inline source, with an ATX closing sequence removed.
fn heading_span(line: &Line, from: usize) -> Span {
    let mut text = line.text[from..].trim_end();
    let stripped = text.trim_end_matches('#');
    if stripped.len() != text.len() && (stripped.is_empty() || stripped.ends_with(' ')) {
        text = stripped.trim_end();
    }
    Span::new(text, line.line, line.col_offset + from + 1)
}

// -- fences ------------------------------------------------------------------

fn fence_at(text: &str) -> Option<(char, usize, usize)> {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &text[indent..];
    let ch = rest.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let run = rest.chars().take_while(|c| *c == ch).count();
    if run < 3 {
        return None;
    }
    Some((ch, run, indent))
}

fn read_fence(lines: &[Line], i: &mut usize) -> Option<Block> {
    let (ch, run, indent) = fence_at(&lines[*i].text)?;
    let open = &lines[*i];
    let info = open.text[indent + run..].trim().to_string();
    // An info string may not contain a backtick when the fence is backticks.
    if ch == '`' && info.contains('`') {
        return None;
    }
    let start_line = open.line;
    let mut body_lines: Vec<String> = Vec::new();
    let mut j = *i + 1;
    while j < lines.len() {
        if let Some((c2, run2, ind2)) = fence_at(&lines[j].text) {
            if c2 == ch && run2 >= run && lines[j].text[ind2 + run2..].trim().is_empty() {
                j += 1;
                *i = j;
                return Some(Block::CodeFence {
                    info,
                    body: join_body(&body_lines),
                    line: start_line,
                });
            }
        }
        let text = &lines[j].text;
        let strip = text.len() - text.trim_start_matches(' ').len();
        body_lines.push(text[strip.min(indent)..].to_string());
        j += 1;
    }
    *i = j;
    Some(Block::CodeFence {
        info,
        body: join_body(&body_lines),
        line: start_line,
    })
}

fn join_body(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        let mut s = lines.join("\n");
        s.push('\n');
        s
    }
}

// -- lists -------------------------------------------------------------------

/// A `>` line. It carries no node — a quote is outside the five block shapes
/// and its lines read as text (spec §6.2) — but it is the one out-of-scope
/// construct that ALWAYS interrupts, so a list item's lazy continuation ends
/// at it. A substrate that let the item swallow it would part company with
/// every CommonMark reader on the second line of a quote.
fn opens_quote(text: &str) -> bool {
    let indent = text.len() - text.trim_start_matches(' ').len();
    indent <= 3 && text[indent..].starts_with('>')
}

pub(crate) fn starts_block(text: &str) -> bool {
    heading_at(text).is_some()
        || fence_at(text).is_some()
        || is_thematic_break(text)
        || list_marker(text).is_some()
}

fn read_list(lines: &[Line], i: &mut usize) -> Block {
    let list_indent = lines[*i].indent();
    let (ordered, _) = list_marker(&lines[*i].text).expect("caller checked");
    let mut items = Vec::new();

    loop {
        let line = &lines[*i];
        let (_, content) = list_marker(&line.text).expect("checked at loop end");
        let source = line.span_from(content);

        let mut item_lines: Vec<Line> = Vec::new();
        let mut j = *i + 1;
        let mut pending_blanks: Vec<Line> = Vec::new();
        while j < lines.len() {
            let candidate = &lines[j];
            if candidate.is_blank() {
                pending_blanks.push(candidate.dedent(0));
                j += 1;
                continue;
            }
            let ind = candidate.indent();
            if ind >= content {
                item_lines.append(&mut pending_blanks);
                item_lines.push(candidate.dedent(content));
                j += 1;
            } else if pending_blanks.is_empty()
                && !starts_block(&candidate.text)
                && !opens_quote(&candidate.text)
            {
                // Lazy continuation of the item's paragraph.
                item_lines.push(candidate.dedent(ind));
                j += 1;
            } else {
                break;
            }
        }

        items.push(ListItem {
            source,
            blocks: parse_blocks(&item_lines),
        });
        *i = j;

        let continues = *i < lines.len()
            && !lines[*i].is_blank()
            && lines[*i].indent() == list_indent
            && list_marker(&lines[*i].text).map(|(o, _)| o) == Some(ordered);
        if !continues {
            break;
        }
    }

    Block::List { ordered, items }
}

// -- paragraphs --------------------------------------------------------------

fn read_paragraph(lines: &[Line], i: &mut usize) -> Block {
    let mut spans = Vec::new();
    while *i < lines.len() {
        let line = &lines[*i];
        if line.is_blank() {
            break;
        }
        if !spans.is_empty() && starts_block(&line.text) {
            break;
        }
        spans.push(line.span_from(line.indent()));
        *i += 1;
    }
    Block::Paragraph { lines: spans }
}
