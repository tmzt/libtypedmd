//! The sigil table and the consumer alias table (spec §2, Part II §1).

/// The seven canonical ASCII sigils. Recognized by EXACT TOKEN MATCH — there
/// are no patterns, and the table is closed.
///
/// A sigil is a TYPE. It appears in exactly two positions and prefixes what it
/// types in both: leading a row (`- [abc] id`) and leading a slot
/// (`{{ [abc] svc.name }}`). It is never a value, never a standalone token,
/// never a tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sigil {
    /// `[abc]`
    String,
    /// `[#]`
    Number,
    /// `[1.]`
    List,
    /// `[-]`
    Link,
    /// `[31]`
    Date,
    /// `[_]`
    Boolean,
    /// `[$]`
    Currency,
}

/// The whole table, in spec order.
pub const SIGILS: [Sigil; 7] = [
    Sigil::String,
    Sigil::Number,
    Sigil::List,
    Sigil::Link,
    Sigil::Date,
    Sigil::Boolean,
    Sigil::Currency,
];

impl Sigil {
    /// The canonical token, without brackets. This is the ONLY spelling the
    /// emitter produces (spec §8.3).
    pub const fn token(self) -> &'static str {
        match self {
            Sigil::String => "abc",
            Sigil::Number => "#",
            Sigil::List => "1.",
            Sigil::Link => "-",
            Sigil::Date => "31",
            Sigil::Boolean => "_",
            Sigil::Currency => "$",
        }
    }

    /// Membership in the table is the WHOLE test for sigil vs tag (spec §2).
    pub fn from_token(token: &str) -> Option<Sigil> {
        SIGILS.iter().copied().find(|s| s.token() == token)
    }

    /// A human name for the type. Not part of any serialization.
    pub const fn type_name(self) -> &'static str {
        match self {
            Sigil::String => "String",
            Sigil::Number => "Number",
            Sigil::List => "List",
            Sigil::Link => "Link",
            Sigil::Date => "Date",
            Sigil::Boolean => "Boolean",
            Sigil::Currency => "Currency",
        }
    }
}

/// What a bracketed token resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Annotation {
    /// A core sigil: the row is a Member.
    Sigil(Sigil),
    /// Anything else: the row is an Object with this tag, carried verbatim.
    Tag(String),
}

/// The consumer's spellings, mapped onto canonical tokens (Part II §1).
///
/// It covers BOTH sigils and tags, and **an alias never changes a node's
/// kind**: a sigil alias resolves to a core sigil (so the row is a Member), a
/// tag alias resolves to a tag (so the row is an Object). The two halves are
/// separate maps precisely so the kind cannot be crossed by a table entry.
///
/// Without a table only the seven canonical sigils match, and every other
/// bracketed token is carried verbatim.
#[derive(Debug, Clone, Default)]
pub struct AliasTable {
    sigils: Vec<(std::string::String, Sigil)>,
    tags: Vec<(std::string::String, std::string::String)>,
}

impl AliasTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.sigils.is_empty() && self.tags.is_empty()
    }

    /// Map a legacy spelling (`⚯`, `o`, …) onto a core sigil. The token is
    /// given WITHOUT brackets, as it appears between them.
    pub fn sigil(&mut self, spelling: impl Into<std::string::String>, sigil: Sigil) -> &mut Self {
        let spelling = spelling.into();
        self.sigils.retain(|(s, _)| s != &spelling);
        self.sigils.push((spelling, sigil));
        self
    }

    /// Map a consumer spelling onto a canonical TAG. Still an Object.
    pub fn tag(
        &mut self,
        spelling: impl Into<std::string::String>,
        canonical: impl Into<std::string::String>,
    ) -> &mut Self {
        let spelling = spelling.into();
        self.tags.retain(|(s, _)| s != &spelling);
        self.tags.push((spelling, canonical.into()));
        self
    }

    /// Resolve a bracketed token.
    ///
    /// Canonical sigils are tested FIRST, so no alias can shadow one and
    /// silently change a document's meaning; then sigil aliases, then tag
    /// aliases, then the token verbatim.
    pub fn resolve(&self, token: &str) -> Annotation {
        if let Some(s) = Sigil::from_token(token) {
            return Annotation::Sigil(s);
        }
        if let Some((_, s)) = self.sigils.iter().find(|(k, _)| k == token) {
            return Annotation::Sigil(*s);
        }
        if let Some((_, canonical)) = self.tags.iter().find(|(k, _)| k == token) {
            return Annotation::Tag(canonical.clone());
        }
        Annotation::Tag(token.to_string())
    }

    /// Resolve a token that may only be a sigil — the slot position, where a
    /// tag is not permitted (spec §3.2).
    pub fn resolve_sigil(&self, token: &str) -> Option<Sigil> {
        Sigil::from_token(token).or_else(|| {
            self.sigils
                .iter()
                .find(|(k, _)| k == token)
                .map(|(_, s)| *s)
        })
    }
}
