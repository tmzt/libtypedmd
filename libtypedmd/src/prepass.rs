//! The lexical pre-pass (spec §6.1).
//!
//! Stream splitting and envelope peeling run BEFORE the Markdown substrate
//! sees any text, which is why a substrate never has to know what `---` means
//! and why `---` can be unambiguously structural. Inside a body a thematic
//! break must be spelled `***` or `___`.

/// One document as the pre-pass carved it: its envelope lines (unparsed, with
/// their 1-based file line numbers) and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDocument {
    pub envelope_lines: Vec<(usize, String)>,
    pub body: String,
    /// 1-based file line of the body's first line, so body-relative
    /// locations can be offset back onto the input.
    pub body_line: usize,
}

/// Is this line a standalone `---`?
fn is_marker(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    indent <= 3 && line.trim() == "---"
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Split the input into documents, peeling each envelope.
///
/// A file with no structural `---` is ONE document with an empty envelope
/// (spec §1), so this never returns an empty vector.
pub fn split_stream(input: &str) -> Vec<RawDocument> {
    let lines: Vec<&str> = input.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();

    let mut docs = Vec::new();
    let mut i = 0usize;

    loop {
        // -- document start -------------------------------------------------
        let mut k = i;
        while k < lines.len() && is_blank(lines[k]) {
            k += 1;
        }

        let mut envelope_lines = Vec::new();
        if k < lines.len() && is_marker(lines[k]) {
            // Rule 2: a structural `---` at document start opens frontmatter
            // IFF a closing `---` arrives before any content. "Before any
            // content" is read here as: before a blank line or end of input.
            // Frontmatter is a run of consecutive non-blank `key: value`
            // lines, so a blank line means the `---` was something else.
            let mut m = k + 1;
            while m < lines.len() && !is_blank(lines[m]) && !is_marker(lines[m]) {
                m += 1;
            }
            if m < lines.len() && is_marker(lines[m]) {
                for (offset, line) in lines[k + 1..m].iter().enumerate() {
                    envelope_lines.push((k + 2 + offset, line.to_string()));
                }
                k = m + 1;
            } else {
                // Not frontmatter. The marker still belongs to the split, not
                // to the body: consume it so the body never starts with one.
                k += 1;
            }
        }

        // -- body -----------------------------------------------------------
        let body_start = k;
        let mut end = lines.len();
        let mut e = body_start;
        while e < lines.len() {
            // Rule 1: structural only at file start or after a blank line.
            // A `---` on the line after a paragraph is a setext underline or
            // ordinary text and stays in the body.
            if is_marker(lines[e]) && e > 0 && is_blank(lines[e - 1]) {
                end = e;
                break;
            }
            e += 1;
        }

        docs.push(RawDocument {
            envelope_lines,
            body: lines[body_start..end].join("\n"),
            body_line: body_start + 1,
        });

        if end < lines.len() {
            i = end + 1;
        } else {
            break;
        }
    }

    docs
}

/// Split one envelope line into `key: value`. `None` when the line has no
/// colon at all — the envelope plane is flat pairs and nothing else.
pub fn envelope_pair(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), value.trim().to_string()))
}
