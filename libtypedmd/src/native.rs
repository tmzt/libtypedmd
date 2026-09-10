//! The native block substrate: a small, dependency-free Markdown block
//! reader implementing [`MarkdownParser`].
//!
//! It reads exactly the five block shapes the trait names and NOTHING else:
//! no emphasis, no links, no HTML, no GFM, no setext headings, no indented
//! code blocks. Inline text is handed on as a byte-exact span.

use crate::substrate::{Block, ListItem, MarkdownParser, Span};

/// The dependency-free substrate. Interchangeable with any other conforming
/// implementation (spec §6.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeParser;

impl MarkdownParser for NativeParser {
    fn parse_body(&self, body: &str) -> Vec<Block> {
        let lines: Vec<Line> = body
            .split('\n')
            .enumerate()
            .map(|(idx, raw)| Line {
                text: raw.strip_suffix('\r').unwrap_or(raw).to_string(),
                line: idx + 1,
                col_offset: 0,
            })
            .collect();
        parse_blocks(&lines)
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
            out.push(Block::Heading { level, source: heading_span(line, from) });
            i += 1;
            continue;
        }
        if marker_at(&line.text).is_some() {
            out.push(read_list(lines, &mut i));
            continue;
        }
        out.push(read_paragraph(lines, &mut i));
    }
    out
}

// -- thematic breaks ---------------------------------------------------------

/// `***` / `___` only. The pre-pass owns `---`; anything spelled that way
/// which survived into a body is treated as a break too, and is dropped —
/// it carries no node.
fn is_thematic_break(text: &str) -> bool {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let trimmed: String = text.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed.len() < 3 {
        return false;
    }
    let first = trimmed.as_bytes()[0];
    matches!(first, b'*' | b'_' | b'-') && trimmed.bytes().all(|b| b == first)
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
    Some(Block::CodeFence { info, body: join_body(&body_lines), line: start_line })
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

/// The marker on this line, if any: `(ordered, byte offset of content)`.
fn marker_at(text: &str) -> Option<(bool, usize)> {
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent > 3 && text.len() == indent {
        return None;
    }
    let rest = &text[indent..];
    let bytes = rest.as_bytes();
    let (ordered, marker_len) = if matches!(bytes.first(), Some(b'-' | b'*' | b'+')) {
        (false, 1)
    } else {
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        match bytes.get(digits) {
            Some(b'.' | b')') => (true, digits + 1),
            _ => return None,
        }
    };
    match bytes.get(marker_len) {
        None => Some((ordered, indent + marker_len + 1)),
        Some(b' ') => {
            let after = &rest[marker_len..];
            let spaces = after.len() - after.trim_start_matches(' ').len();
            // A run of 5+ spaces is not part of the marker (CommonMark).
            let spaces = if spaces > 4 { 1 } else { spaces };
            Some((ordered, indent + marker_len + spaces))
        }
        _ => None,
    }
}

pub(crate) fn starts_block(text: &str) -> bool {
    heading_at(text).is_some()
        || fence_at(text).is_some()
        || is_thematic_break(text)
        || marker_at(text).is_some()
}

fn read_list(lines: &[Line], i: &mut usize) -> Block {
    let list_indent = lines[*i].indent();
    let (ordered, _) = marker_at(&lines[*i].text).expect("caller checked");
    let mut items = Vec::new();

    loop {
        let line = &lines[*i];
        let (_, content) = marker_at(&line.text).expect("checked at loop end");
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
                && !item_lines.is_empty()
                && !starts_block(&candidate.text)
            {
                // Lazy continuation of the item's paragraph.
                item_lines.push(candidate.dedent(ind));
                j += 1;
            } else {
                break;
            }
        }

        items.push(ListItem { source, blocks: parse_blocks(&item_lines) });
        *i = j;

        let continues = *i < lines.len()
            && !lines[*i].is_blank()
            && lines[*i].indent() == list_indent
            && marker_at(&lines[*i].text).map(|(o, _)| o) == Some(ordered);
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
