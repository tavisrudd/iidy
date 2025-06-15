use std::fmt;
use url::Url;

use super::ast::Position;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub location: Option<ParseLocation>,
}

#[derive(Debug, Clone)]
pub struct ParseLocation {
    pub uri: Url,
    pub start: Position,
    pub end: Position,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(loc) = &self.location {
            write!(f, "Parse error at {}:{}:{}: {}", 
                   loc.uri, loc.start.line + 1, loc.start.character + 1, self.message)
        } else {
            write!(f, "Parse error: {}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: None,
        }
    }

    pub fn with_location(message: impl Into<String>, uri: Url, start: Position, end: Position) -> Self {
        Self {
            message: message.into(),
            location: Some(ParseLocation { uri, start, end }),
        }
    }
}