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
        // Just display the message - it already contains location info in the correct format
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, Box<ParseError>>;

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

/// Parse mode for future extensibility - not currently used but planned for LSP integration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ParseMode {
    StrictValidation, // Current behavior - stop on first error
    CollectAll,       // Collect all errors without AST building
                      // Future: BestEffort, // Build partial AST where possible (Phase 3)
}

pub mod error_codes {
    pub const SYNTAX_ERROR: &str = "ERR_1001";
    pub const UNKNOWN_TAG: &str = "ERR_4001";
    pub const MISSING_FIELD: &str = "ERR_4002";
    pub const INVALID_TYPE: &str = "ERR_4003";
    pub const INVALID_FORMAT: &str = "ERR_4004";
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

impl Default for ParseDiagnostics {
    fn default() -> Self {
        Self::new()
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
