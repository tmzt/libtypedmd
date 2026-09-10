//! The error model (spec §7).
//!
//! A LENIENT parse returns a graph unconditionally. Only a STRICT parse can
//! fail, and it returns the FIRST error: message, location, and the offending
//! claim record.

use crate::model::{Claim, Location};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub location: Location,
    /// The claim the row made and could not honour. `None` for errors that
    /// are not about a node's annotation (a duplicate key, an unclosed slot,
    /// a malformed envelope line).
    pub claim: Option<Claim>,
}

impl ParseError {
    pub fn new(message: impl Into<String>, location: Location) -> Self {
        Self { message: message.into(), location, claim: None }
    }

    pub fn with_claim(mut self, claim: Claim) -> Self {
        self.claim = Some(claim);
        self
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (document {}, line {}, column {})",
            self.message, self.location.document, self.location.line, self.location.column
        )
    }
}

impl std::error::Error for ParseError {}
