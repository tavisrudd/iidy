use std::fmt;
use url::Url;

use super::ast::Position;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub location: Option<ParseLocation>,
    pub code: Option<String>,
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
            write!(
                f,
                "Parse error at {}:{}:{}: {}",
                loc.uri,
                loc.start.line + 1,
                loc.start.character + 1,
                self.message
            )
        } else {
            write!(f, "Parse error: {}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone)]
pub struct ParseWarning {
    pub message: String,
    pub location: Option<ParseLocation>,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParseDiagnostics {
    pub errors: Vec<ParseError>,
    pub warnings: Vec<ParseWarning>,
    pub parse_successful: bool,
}

#[derive(Debug, Clone)]
pub enum ParseMode {
    StrictValidation, // Current behavior - stop on first error
    CollectAll,       // Collect all errors without AST building
                      // Future: BestEffort, // Build partial AST where possible (Phase 3)
}

pub mod error_codes {
    pub const SYNTAX_ERROR: &str = "IY1001";
    pub const UNKNOWN_TAG: &str = "IY4001";
    pub const MISSING_FIELD: &str = "IY4002";
    pub const INVALID_TYPE: &str = "IY4003";
    pub const INVALID_FORMAT: &str = "IY4004";
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: None,
            code: None,
        }
    }

    pub fn with_location(
        message: impl Into<String>,
        uri: Url,
        start: Position,
        end: Position,
    ) -> Self {
        Self {
            message: message.into(),
            location: Some(ParseLocation { uri, start, end }),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

impl ParseDiagnostics {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            parse_successful: true,
        }
    }

    pub fn add_error(&mut self, error: ParseError) {
        self.errors.push(error);
        self.parse_successful = false;
    }

    pub fn add_warning(&mut self, warning: ParseWarning) {
        self.warnings.push(warning);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

impl ParseWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: None,
            code: None,
        }
    }

    pub fn with_location(
        message: impl Into<String>,
        uri: Url,
        start: Position,
        end: Position,
    ) -> Self {
        Self {
            message: message.into(),
            location: Some(ParseLocation { uri, start, end }),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}
