//! The row grammar (spec §3): annotation recognition, head delimitation,
//! inline properties, sinks and slots.
//!
//! This runs on a row's VERBATIM source, after the block parse and before any
//! inline Markdown interpretation, so a sigil can never be consumed as a
//! CommonMark link (spec §6.2).

use crate::error::ParseError;
use crate::model::{
    Claim, ClaimShape, Fallback, Leaf, LeafForm, Location, Member, Node, NodeKind, Object,
    Placement, Slot, Text, TextPart,
};
use crate::sigil::{AliasTable, Annotation, Sigil};

/// What one row produced.
pub struct ScannedRow {
    pub kind: NodeKind,
    /// Inline `name=value` members, as nodes with [`Placement::Inline`].
    /// They are the parent's FIRST children, in scanned order.
    pub inline: Vec<Node>,
    /// Problems a lenient parse absorbed and a strict parse must reject.
    pub issues: Vec<ParseError>,
}

impl ScannedRow {
    fn new(kind: NodeKind) -> Self {
        Self { kind, inline: Vec::new(), issues: Vec::new() }
    }
}

/// Which head terminators are live in this position (spec §3.1).
#[derive(Clone, Copy)]
struct Stops {
    /// A bare identifier immediately followed by `=`.
    props: bool,
    /// An unquoted ` -> `.
    arrow: bool,
    /// An unquoted `:` followed by whitespace or end of line.
    colon: bool,
    /// A leading `"` runs to the closing quote, terminators suspended.
    quoted_prefix: bool,
}

const HEAD_MEMBER: Stops = Stops { props: true, arrow: true, colon: true, quoted_prefix: true };
const HEAD_OBJECT: Stops = Stops { props: true, arrow: true, colon: false, quoted_prefix: true };
const RHS: Stops = Stops { props: false, arrow: true, colon: false, quoted_prefix: false };

/// Scan one row's source into a node kind.
///
/// `src` is the row's verbatim text with the marker and its following spaces
/// already removed; `loc` is the location of its first byte.
pub fn scan_row(src: &str, is_heading: bool, aliases: &AliasTable, loc: Location) -> ScannedRow {
    let src = src.trim();
    if src.is_empty() {
        return if is_heading {
            ScannedRow::new(NodeKind::Object(Object::default()))
        } else {
            ScannedRow::new(NodeKind::Leaf(Leaf { text: String::new(), form: LeafForm::Row }))
        };
    }

    match scan_annotation(src, aliases) {
        Annot::Malformed(token) => ScannedRow::new(NodeKind::Fallback(Fallback {
            text: src.to_string(),
            claim: Claim { shape: ClaimShape::Sigil, token, attrs: Vec::new(), location: loc },
        })),
        Annot::Resolved(Annotation::Sigil(sigil), rest) => {
            member_row(src, rest, Some(sigil), aliases, loc)
        }
        Annot::Resolved(Annotation::Tag(tag), rest) => {
            object_row(src, rest, Some(tag), aliases, loc)
        }
        Annot::Absent => {
            // Rule 2: no annotation but a colon-RHS is a Member; no
            // annotation and no colon is a Leaf on a list row and an object
            // head restatement on a heading row.
            let mut scratch = Vec::new();
            let (_, end) = scan_text(src, 0, HEAD_MEMBER, aliases, loc, &mut scratch);
            let has_colon = src[end..].starts_with(':');
            if has_colon {
                member_row(src, 0, None, aliases, loc)
            } else if is_heading {
                object_row(src, 0, None, aliases, loc)
            } else {
                ScannedRow::new(NodeKind::Leaf(Leaf {
                    text: whole_quoted(src).unwrap_or_else(|| src.to_string()),
                    form: LeafForm::Row,
                }))
            }
        }
    }
}

// -- annotation --------------------------------------------------------------

enum Annot {
    Absent,
    /// The resolved annotation and the byte offset where the head starts.
    Resolved(Annotation, usize),
    /// A row that LED WITH A BRACKET and could not be resolved. Its claim
    /// shape is `Sigil`, so strict mode rejects it.
    Malformed(Option<String>),
}

/// At most one annotation, recognized only as the first token after the
/// marker (spec §3, rule 1). Later bracketed tokens are ordinary text.
fn scan_annotation(src: &str, aliases: &AliasTable) -> Annot {
    if !src.starts_with('[') {
        return Annot::Absent;
    }
    let Some(close) = src.find(']') else {
        return Annot::Malformed(Some(src[1..].to_string()));
    };
    let token = &src[1..close];
    let after = &src[close + 1..];
    let token_ok = !token.is_empty() && !token.chars().any(|c| c.is_whitespace() || c == '[');
    let boundary = after.is_empty() || after.starts_with(char::is_whitespace);
    if token_ok && boundary {
        let head_at = close + 1 + (after.len() - after.trim_start().len());
        Annot::Resolved(aliases.resolve(token), head_at)
    } else {
        Annot::Malformed(Some(token.to_string()))
    }
}

// -- member rows -------------------------------------------------------------

fn member_row(
    src: &str,
    from: usize,
    sigil: Option<Sigil>,
    aliases: &AliasTable,
    loc: Location,
) -> ScannedRow {
    let mut issues = Vec::new();
    let (head, end) = scan_text(src, from, HEAD_MEMBER, aliases, loc, &mut issues);
    let name = head.render();

    let mut rhs = None;
    let mut sink = None;

    if src[end..].starts_with(':') {
        let after_colon = end + 1;
        let rest = &src[after_colon..];
        let trimmed = rest.trim_start();
        if trimmed == "->" || trimmed.starts_with("-> ") {
            // An empty RHS with a sink: `- name: -> path`.
            sink = Some(trimmed[2..].trim().to_string());
        } else {
            let value_at = after_colon + (rest.len() - trimmed.len());
            let (text, stop) = scan_text(src, value_at, RHS, aliases, loc, &mut issues);
            if !text.is_empty() {
                rhs = Some(text);
            }
            if at_arrow(src, stop) {
                sink = Some(src[stop + 3..].trim().to_string());
            }
        }
    } else if at_arrow(src, end) {
        sink = Some(src[end + 3..].trim().to_string());
    } else if end < src.len() {
        // The head stopped at an inline property, but the member-row tail is
        // a colon-RHS (spec §3, `tail`). The row claimed attributes it cannot
        // carry, so it falls back with `shape = Attrs`.
        let attrs = scan_attr_claim(src, end, aliases);
        return ScannedRow::new(NodeKind::Fallback(Fallback {
            text: src.to_string(),
            claim: Claim {
                shape: ClaimShape::Attrs,
                token: sigil.map(|s| s.token().to_string()),
                attrs,
                location: loc,
            },
        }));
    }

    let mut row = ScannedRow::new(NodeKind::Member(Member {
        name,
        sigil,
        rhs,
        sink,
        placement: Placement::Row,
    }));
    row.issues = issues;
    row
}

// -- object rows -------------------------------------------------------------

fn object_row(
    src: &str,
    from: usize,
    tag: Option<String>,
    aliases: &AliasTable,
    loc: Location,
) -> ScannedRow {
    let mut issues = Vec::new();
    let (head, end) = scan_text(src, from, HEAD_OBJECT, aliases, loc, &mut issues);

    let mut props: Vec<(String, Text)> = Vec::new();
    let mut sink = None;
    let mut p = end;
    loop {
        p += leading_spaces(&src[p..]);
        if p >= src.len() {
            break;
        }
        if src[p..].starts_with("->") && (src[p..] == *"->" || src[p..].starts_with("-> ")) {
            sink = Some(src[p + 2..].trim().to_string());
            break;
        }
        match read_prop(src, p, aliases) {
            Some((name, value, next)) => {
                props.push((name, value));
                p = next;
            }
            None => {
                let mut attrs: Vec<(String, String)> =
                    props.iter().map(|(n, v)| (n.clone(), v.render())).collect();
                attrs.extend(scan_attr_claim(src, p, aliases));
                return ScannedRow::new(NodeKind::Fallback(Fallback {
                    text: src.to_string(),
                    claim: Claim { shape: ClaimShape::Attrs, token: tag, attrs, location: loc },
                }));
            }
        }
    }

    let mut row = ScannedRow::new(NodeKind::Object(Object {
        tag,
        head,
        sink,
        carrier: crate::model::Carrier::Row,
    }));
    row.inline = props
        .into_iter()
        .map(|(name, value)| {
            Node::member(name).with_rhs(value).with_placement(Placement::Inline)
        })
        .collect();
    row.issues = issues;
    row
}

/// Best-effort `name=value` scan for a claim record: what the row APPEARED to
/// assert, in order, so the report names something the author wrote.
fn scan_attr_claim(src: &str, from: usize, aliases: &AliasTable) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut p = from;
    while p < src.len() {
        p += leading_spaces(&src[p..]);
        match read_prop(src, p, aliases) {
            Some((name, value, next)) => {
                out.push((name, value.render()));
                p = next;
            }
            None => {
                let Some(name_end) = ident_before_eq(src, p) else {
                    // Not a property start; step past this token.
                    let step = src[p..].find(' ').map(|i| p + i + 1).unwrap_or(src.len());
                    p = step;
                    continue;
                };
                out.push((src[p..name_end].to_string(), src[name_end + 1..].trim().to_string()));
                break;
            }
        }
    }
    out
}

/// `name` followed immediately by `=`, at `p`. Returns the index of the `=`.
fn ident_before_eq(src: &str, p: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let first = *bytes.get(p)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut q = p + 1;
    while q < bytes.len()
        && (bytes[q].is_ascii_alphanumeric() || bytes[q] == b'_' || bytes[q] == b'-')
    {
        q += 1;
    }
    (bytes.get(q) == Some(&b'=')).then_some(q)
}

/// One `name=value` pair (spec §3, rules 1 and 4).
fn read_prop(src: &str, p: usize, aliases: &AliasTable) -> Option<(String, Text, usize)> {
    let eq = ident_before_eq(src, p)?;
    let name = src[p..eq].to_string();
    let (value, next) = read_value(src, eq + 1, aliases)?;
    Some((name, value, next))
}

/// A value is a bare token, a double-quoted string, or a slot.
fn read_value(src: &str, p: usize, aliases: &AliasTable) -> Option<(Text, usize)> {
    if src[p..].starts_with('"') {
        let (content, next) = read_quoted(src, p)?;
        return Some((Text::literal(content), next));
    }
    if src[p..].starts_with("{{") {
        let (slot, next) = read_slot(src, p, aliases)?;
        return Some((Text::slot(slot), next));
    }
    let end = src[p..].find(' ').map(|i| p + i).unwrap_or(src.len());
    let token = &src[p..end];
    if token.is_empty() || token.contains('"') || token.contains('=') || token.contains("{{") {
        return None;
    }
    Some((Text::literal(token), end))
}

// -- text, heads and slots ---------------------------------------------------

fn leading_spaces(src: &str) -> usize {
    src.len() - src.trim_start_matches(' ').len()
}

/// An unquoted ` -> ` (space, arrow, space), or ` ->` at end of line.
fn at_arrow(src: &str, p: usize) -> bool {
    let rest = &src[p..];
    rest == " ->" || rest.starts_with(" -> ")
}

/// An unquoted `:` followed by whitespace or end of line. A colon NOT
/// followed by whitespace (`http://…`) does not end a head (spec §3.1).
fn at_colon(src: &str, p: usize) -> bool {
    let bytes = src.as_bytes();
    bytes.get(p) == Some(&b':')
        && bytes.get(p + 1).map(|b| b.is_ascii_whitespace()).unwrap_or(true)
}

/// The start of inline properties: an unquoted bare identifier immediately
/// followed by `=`, at a word boundary.
fn at_prop(src: &str, p: usize) -> bool {
    let at_boundary = p == 0 || src.as_bytes()[p - 1] == b' ';
    at_boundary && ident_before_eq(src, p).is_some()
}

/// Scan opaque text from `from` until a live terminator. Returns the text and
/// the byte offset it stopped at.
fn scan_text(
    src: &str,
    from: usize,
    stops: Stops,
    aliases: &AliasTable,
    loc: Location,
    issues: &mut Vec<ParseError>,
) -> (Text, usize) {
    let mut parts: Vec<TextPart> = Vec::new();
    let mut lit = String::new();
    let mut p = from;

    if stops.quoted_prefix && src[p..].starts_with('"') {
        if let Some((content, next)) = read_quoted(src, p) {
            lit.push_str(&content);
            p = next;
        }
    }

    while p < src.len() {
        if stops.props && at_prop(src, p) {
            break;
        }
        if stops.arrow && at_arrow(src, p) {
            break;
        }
        if stops.colon && at_colon(src, p) {
            break;
        }
        if src[p..].starts_with("\\{{") {
            lit.push_str("{{");
            p += 3;
            continue;
        }
        if src[p..].starts_with("{{") {
            match read_slot(src, p, aliases) {
                Some((slot, next)) => {
                    if !lit.is_empty() {
                        parts.push(TextPart::Literal(std::mem::take(&mut lit)));
                    }
                    parts.push(TextPart::Slot(slot));
                    p = next;
                    continue;
                }
                None => {
                    // Lenient: literal text. Strict: an error (spec §3.2).
                    issues.push(ParseError::new(
                        "unclosed slot: `{{` with no closing `}}` on the row",
                        Location { column: loc.column + p, ..loc },
                    ));
                }
            }
        }
        let ch = src[p..].chars().next().expect("in bounds");
        lit.push(ch);
        p += ch.len_utf8();
    }

    let trimmed = lit.trim_end();
    if !trimmed.is_empty() {
        parts.push(TextPart::Literal(trimmed.to_string()));
    }
    (Text::from_parts(parts), p)
}

/// A list row that is ENTIRELY one double-quoted string: its value is the
/// unquoted content.
///
/// The spec does not name this case; it is what makes a leaf whose text
/// contains `a: b` expressible. Without it the emitter has no spelling that
/// reads back as a leaf, and the node silently becomes a member on the next
/// parse — a kind change caused by emission, which is exactly what guarantee
/// 1 forbids.
fn whole_quoted(src: &str) -> Option<String> {
    let (content, next) = read_quoted(src, 0)?;
    (next == src.len()).then_some(content)
}

/// A double-quoted string with `\"` and `\\` escapes. `None` if unterminated.
fn read_quoted(src: &str, p: usize) -> Option<(String, usize)> {
    let bytes = src.as_bytes();
    if bytes.get(p) != Some(&b'"') {
        return None;
    }
    let mut out = String::new();
    let mut q = p + 1;
    while q < src.len() {
        match bytes[q] {
            b'"' => return Some((out, q + 1)),
            b'\\' if q + 1 < src.len() && matches!(bytes[q + 1], b'"' | b'\\') => {
                out.push(bytes[q + 1] as char);
                q += 2;
            }
            _ => {
                let ch = src[q..].chars().next().expect("in bounds");
                out.push(ch);
                q += ch.len_utf8();
            }
        }
    }
    None
}

/// A `{{ }}` slot. Must open and close on one line and never nests. The
/// optional sigil must be the FIRST token inside the braces; the rest is
/// opaque expression text (spec §3.2). A bracketed token that is not a sigil
/// is part of the expression — slots take types, not tags.
fn read_slot(src: &str, p: usize, aliases: &AliasTable) -> Option<(Slot, usize)> {
    let open = p + 2;
    let close = src[open..].find("}}")? + open;
    let inner = src[open..close].trim();
    let mut sigil = None;
    let mut expr = inner;
    if let Some(rest) = inner.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let token = &rest[..end];
            let after = &rest[end + 1..];
            if after.is_empty() || after.starts_with(char::is_whitespace) {
                if let Some(s) = aliases.resolve_sigil(token) {
                    sigil = Some(s);
                    expr = after.trim();
                }
            }
        }
    }
    Some((Slot { sigil, expr: expr.to_string() }, close + 2))
}

// -- shared with the emitter -------------------------------------------------

/// Does any live terminator occur anywhere in `s`? The emitter asks this to
/// decide whether a head, a name or a leaf must be quoted.
///
/// This is deliberately the SAME predicate the scanner uses, called on the
/// same string: a control's hit region and its drawn geometry from one
/// formula. If quoting and scanning were two spellings of "unsafe" they would
/// drift, and the drift would show up only as a round trip that silently
/// changed a node's kind.
pub(crate) fn stops_anywhere(s: &str, colon: bool) -> bool {
    (0..s.len()).any(|p| {
        s.is_char_boundary(p) && (at_prop(s, p) || at_arrow(s, p) || (colon && at_colon(s, p)))
    })
}

/// Does a `:` terminator occur anywhere in `s`?
pub(crate) fn colon_anywhere(s: &str) -> bool {
    (0..s.len()).any(|p| s.is_char_boundary(p) && at_colon(s, p))
}
