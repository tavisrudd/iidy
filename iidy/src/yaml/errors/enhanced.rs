use std::fmt;
use crate::yaml::errors::ErrorId;

/// Source location information for precise error reporting
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub yaml_path: String,
}

impl SourceLocation {
    pub fn new(file: &str, line: usize, column: usize, yaml_path: &str) -> Self {
        Self {
            file: file.to_string(),
            line,
            column,
            yaml_path: yaml_path.to_string(),
        }
    }
    
    pub fn unknown(file: &str) -> Self {
        Self {
            file: file.to_string(),
            line: 0,
            column: 0,
            yaml_path: "<unknown>".to_string(),
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}:{}", self.file, self.line, self.column)
        } else {
            write!(f, "{}", self.file)
        }
    }
}

/// Enhanced preprocessing error with error IDs and rich context
#[derive(Debug)]
pub enum EnhancedPreprocessingError {
    VariableNotFound {
        error_id: ErrorId,
        variable: String,
        location: SourceLocation,
        available_vars: Vec<String>,
        suggestions: Vec<String>,
    },
    
    TypeMismatch {
        error_id: ErrorId,
        expected: String,
        found: String,
        location: SourceLocation,
        context: String,
        help: Option<String>,
    },
    
    MissingRequiredField {
        error_id: ErrorId,
        tag_name: String,
        missing_field: String,
        location: SourceLocation,
        required_fields: Vec<String>,
    },
    
    ImportError {
        error_id: ErrorId,
        import_type: String,
        location_str: String,
        location: SourceLocation,
        underlying_error: String,
        suggestions: Vec<String>,
    },
    
    HandlebarsError {
        error_id: ErrorId,
        template: String,
        location: SourceLocation,
        underlying_error: String,
        available_helpers: Vec<String>,
    },
}

impl EnhancedPreprocessingError {
    pub fn error_id(&self) -> ErrorId {
        match self {
            Self::VariableNotFound { error_id, .. } => *error_id,
            Self::TypeMismatch { error_id, .. } => *error_id,
            Self::MissingRequiredField { error_id, .. } => *error_id,
            Self::ImportError { error_id, .. } => *error_id,
            Self::HandlebarsError { error_id, .. } => *error_id,
        }
    }
    
    pub fn location(&self) -> &SourceLocation {
        match self {
            Self::VariableNotFound { location, .. } => location,
            Self::TypeMismatch { location, .. } => location,
            Self::MissingRequiredField { location, .. } => location,
            Self::ImportError { location, .. } => location,
            Self::HandlebarsError { location, .. } => location,
        }
    }
    
    /// Display error with clean, user-friendly formatting
    pub fn display_with_context(&self, source_lines: Option<&[String]>) -> String {
        let mut output = String::new();
        let loc = self.location();
        let use_color = std::env::var("NO_COLOR").is_err() && atty::is(atty::Stream::Stderr);
        
        // Color codes
        let red = if use_color { "\x1b[31m" } else { "" };
        let bold_red = if use_color { "\x1b[1;31m" } else { "" };
        let _yellow = if use_color { "\x1b[33m" } else { "" };
        let cyan = if use_color { "\x1b[36m" } else { "" };
        let blue_grey = if use_color { "\x1b[38;5;245m" } else { "" }; // lighter grey for source context
        let light_blue = if use_color { "\x1b[38;5;75m" } else { "" }; // light blue for help text
        let grey = if use_color { "\x1b[90m" } else { "" }; // grey for line numbers
        let reset = if use_color { "\x1b[0m" } else { "" };
        
        // Error header - concise and scannable
        let error_type = match self {
            Self::VariableNotFound { .. } => "Variable error",
            Self::TypeMismatch { .. } => "Type error", 
            Self::MissingRequiredField { .. } => "Missing field error",
            Self::ImportError { .. } => "Import error",
            Self::HandlebarsError { .. } => "Template error",
        };
        
        let short_message = match self {
            Self::VariableNotFound { variable, .. } => format!("'{}' not found", variable),
            Self::TypeMismatch { expected, found, .. } => format!("expected {}, found {}", expected, found),
            Self::MissingRequiredField { missing_field, tag_name, .. } => format!("'{}' missing in {} tag", missing_field, tag_name),
            Self::ImportError { import_type, location_str, .. } => format!("{} import failed: {}", import_type, location_str),
            Self::HandlebarsError { template, .. } => format!("template error in: {}", template),
        };
        
        output.push_str(&format!("{}{}{}: {} @ {}{}{} {}(errno: {}){}\n", 
            bold_red, error_type, reset, short_message,
            cyan, loc, reset,
            grey, self.error_id().code(), reset
        ));
        
        // Add specific guidance on next line
        let guidance = match self {
            Self::VariableNotFound { .. } => "variable not defined in current scope",
            Self::TypeMismatch { .. } => "data type mismatch",
            Self::MissingRequiredField { .. } => "required field missing",
            Self::ImportError { .. } => "import failed",
            Self::HandlebarsError { .. } => "template syntax error",
        };
        
        output.push_str(&format!("{}  -> {}{}\n", light_blue, guidance, reset));
        
        // Show source context prominently (helps user find the problem quickly)
        if let Some(lines) = source_lines {
            if loc.line > 0 && loc.line <= lines.len() {
                output.push_str("\n");
                
                // Show line before for context (if available) - in blue-grey with grey line number
                if loc.line > 1 {
                    let prev_line = &lines[loc.line - 2];
                    output.push_str(&format!("{}{:4}{} | {}{}{}\n", grey, loc.line - 1, reset, blue_grey, prev_line, reset));
                }
                
                // Show the problematic line with highlighting - make line number red to draw attention
                let line_content = &lines[loc.line - 1];
                output.push_str(&format!("{}{:4}{} | {}\n", red, loc.line, reset, line_content));
                
                // Show caret pointing to the error with color
                if loc.column > 0 && loc.column <= line_content.len() {
                    let spaces = " ".repeat(loc.column - 1); // column offset
                    let span_len = self.error_span_length().min(line_content.len() - loc.column + 1);
                    let carets = "^".repeat(span_len.max(1));
                    
                    // Use red color for the error highlight and blue-grey for description
                    output.push_str(&format!("     | {}{}{}{} {}{}{}\n", 
                        spaces, red, carets, reset, blue_grey, self.inline_description(), reset));
                }
                
                // Show line after for context (if available) - in blue-grey with grey line number
                if loc.line < lines.len() {
                    let next_line = &lines[loc.line];
                    output.push_str(&format!("{}{:4}{} | {}{}{}\n", grey, loc.line + 1, reset, blue_grey, next_line, reset));
                }
                
                output.push_str("\n");
            }
        }
        
        // Most important information - specific help for this error (in light blue, no prefixes)
        match self {
            Self::VariableNotFound { suggestions, available_vars, .. } => {
                // Show suggestions first (most actionable)
                if !suggestions.is_empty() {
                    output.push_str(&format!("{}   did you mean '{}'?{}\n", light_blue, suggestions.join("' or '"), reset));
                }
                
                // Show available variables (scan-friendly, one line) - sort for stable output
                if !available_vars.is_empty() {
                    let mut sorted_vars = available_vars.clone();
                    sorted_vars.sort();
                    output.push_str(&format!("{}   available variables: {}{}\n", light_blue, sorted_vars.join(", "), reset));
                }
            },
            Self::MissingRequiredField { missing_field, tag_name, .. } => {
                // Show the specific fix needed
                output.push_str(&format!("{}   add '{}' field to {} tag{}\n", light_blue, missing_field, tag_name, reset));
                
                // Show example if available
                let help_messages = self.help_messages();
                if let Some(help) = help_messages.first() {
                    if help.starts_with("example:") {
                        output.push_str(&format!("{}   {}{}\n", light_blue, help, reset));
                    }
                }
            },
            Self::TypeMismatch { expected, found, help, .. } => {
                output.push_str(&format!("{}   expected {}, found {}{}\n", light_blue, expected, found, reset));
                if let Some(type_help) = help {
                    output.push_str(&format!("{}   {}{}\n", light_blue, type_help, reset));
                }
            },
            _ => {
                // For other error types, show first help message if available
                let help_messages = self.help_messages();
                if let Some(help) = help_messages.first() {
                    output.push_str(&format!("{}   {}{}\n", light_blue, help, reset));
                }
            }
        }
        
        // Reference to detailed help (compact, in light blue)
        output.push_str(&format!("\n{}   For more info: iidy explain {}{}\n", light_blue, self.error_id().code(), reset));
        
        output
    }
        
    fn inline_description(&self) -> String {
        match self {
            Self::VariableNotFound { .. } => "variable not defined".to_string(),
            Self::TypeMismatch { expected, .. } => format!("expected {}", expected),
            Self::MissingRequiredField { missing_field, .. } => format!("missing '{}'", missing_field),
            Self::ImportError { .. } => "import failed".to_string(),
            Self::HandlebarsError { .. } => "template error".to_string(),
        }
    }
    
    fn error_span_length(&self) -> usize {
        match self {
            Self::VariableNotFound { variable, .. } => variable.len() + 3, // "!$ " prefix
            Self::MissingRequiredField { tag_name, .. } => tag_name.len() + 2, // "!$" prefix
            _ => 8, // Default span length
        }
    }
        
    fn help_messages(&self) -> Vec<String> {
        let mut help = Vec::new();
        
        match self {
            Self::VariableNotFound { available_vars, suggestions, .. } => {
                if !available_vars.is_empty() {
                    let mut sorted_vars = available_vars.clone();
                    sorted_vars.sort();
                    help.push(format!("available variables in this scope: {}", sorted_vars.join(", ")));
                }
                for suggestion in suggestions {
                    help.push(format!("did you mean '{}'?", suggestion));
                }
            },
            Self::TypeMismatch { help: type_help, .. } => {
                if let Some(h) = type_help {
                    help.push(h.clone());
                }
            },
            Self::MissingRequiredField { missing_field, tag_name, .. } => {
                help.push(format!("add a '{}' field to the {} tag", missing_field, tag_name));
                match missing_field.as_str() {
                    "template" => help.push("example: template: \"{{item}}\"".to_string()),
                    "items" => help.push("example: items: [1, 2, 3]".to_string()),
                    "test" => help.push("example: test: true".to_string()),
                    _ => {}
                }
            },
            Self::ImportError { suggestions, .. } => {
                help.extend(suggestions.clone());
            },
            Self::HandlebarsError { available_helpers, .. } => {
                if !available_helpers.is_empty() {
                    help.push(format!("available helpers: {}", available_helpers.join(", ")));
                }
            },
        }
        
        help
    }
}

impl fmt::Display for EnhancedPreprocessingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_with_context(None))
    }
}

impl std::error::Error for EnhancedPreprocessingError {}

/// Helper functions for creating enhanced errors
impl EnhancedPreprocessingError {
    pub fn variable_not_found(
        variable: &str,
        location: SourceLocation,
        available_vars: Vec<String>,
    ) -> Self {
        let suggestions = fuzzy_match_variables(variable, &available_vars);
        
        Self::VariableNotFound {
            error_id: ErrorId::VariableNotFound,
            variable: variable.to_string(),
            location,
            available_vars,
            suggestions,
        }
    }
    
    pub fn type_mismatch(
        expected: &str,
        found: &str,
        location: SourceLocation,
        context: &str,
    ) -> Self {
        let help = generate_type_conversion_help(expected, found);
        
        Self::TypeMismatch {
            error_id: ErrorId::TypeMismatchInOperation,
            expected: expected.to_string(),
            found: found.to_string(),
            location,
            context: context.to_string(),
            help,
        }
    }
    
    pub fn missing_required_field(
        tag_name: &str,
        missing_field: &str,
        location: SourceLocation,
        required_fields: Vec<String>,
    ) -> Self {
        Self::MissingRequiredField {
            error_id: ErrorId::MissingRequiredTagField,
            tag_name: tag_name.to_string(),
            missing_field: missing_field.to_string(),
            location,
            required_fields,
        }
    }
}

/// Simple fuzzy matching for variable suggestions
fn fuzzy_match_variables(target: &str, available: &Vec<String>) -> Vec<String> {
    let mut suggestions = Vec::new();
    
    for var in available {
        let distance = levenshtein_distance(target, var);
        // Suggest if edit distance is small relative to string length
        if distance <= (target.len().max(var.len()) / 3).max(1) {
            suggestions.push(var.clone());
        }
    }
    
    // Sort by similarity (shorter distance first)
    suggestions.sort_by_key(|var| levenshtein_distance(target, var));
    
    // Return top 3 suggestions
    suggestions.truncate(3);
    suggestions
}

/// Simple Levenshtein distance calculation
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let m = s1_chars.len();
    let n = s2_chars.len();
    
    if m == 0 { return n; }
    if n == 0 { return m; }
    
    let mut matrix = vec![vec![0; n + 1]; m + 1];
    
    for i in 0..=m { matrix[i][0] = i; }
    for j in 0..=n { matrix[0][j] = j; }
    
    for i in 1..=m {
        for j in 1..=n {
            let cost = if s1_chars[i-1] == s2_chars[j-1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i-1][j] + 1)
                .min(matrix[i][j-1] + 1)
                .min(matrix[i-1][j-1] + cost);
        }
    }
    
    matrix[m][n]
}

/// Generate helpful suggestions for type conversion
fn generate_type_conversion_help(expected: &str, found: &str) -> Option<String> {
    match (expected, found) {
        ("array", "string") => Some("try using !$split to convert a string to an array".to_string()),
        ("string", "array") => Some("try using !$join to convert an array to a string".to_string()),
        ("object", "string") => Some("try using !$parseJson or !$parseYaml to parse the string".to_string()),
        ("string", "object") => Some("try using !$toJsonString or !$toYamlString to serialize the object".to_string()),
        (_, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_source_location_display() {
        let loc = SourceLocation::new("test.yaml", 5, 12, "<root>.config");
        assert_eq!(loc.to_string(), "test.yaml:5:12");
        
        let unknown_loc = SourceLocation::unknown("test.yaml");
        assert_eq!(unknown_loc.to_string(), "test.yaml");
    }
    
    #[test]
    fn test_variable_not_found_error() {
        let location = SourceLocation::new("test.yaml", 3, 9, "<root>.result");
        let available_vars = vec!["app_name".to_string(), "config".to_string()];
        
        let error = EnhancedPreprocessingError::variable_not_found(
            "app_nme", // typo
            location,
            available_vars,
        );
        
        assert_eq!(error.error_id(), ErrorId::VariableNotFound);
        
        let display = error.to_string();
        assert!(display.contains("IY2001"));
        assert!(display.contains("'app_nme' not found"));
        assert!(display.contains("did you mean 'app_name'?"));
    }
    
    #[test]
    fn test_fuzzy_matching() {
        let available = vec![
            "app_name".to_string(),
            "application_name".to_string(), 
            "config".to_string(),
            "database_url".to_string(),
        ];
        
        let suggestions = fuzzy_match_variables("app_nme", &available);
        assert!(suggestions.contains(&"app_name".to_string()));
        assert!(!suggestions.contains(&"database_url".to_string()));
    }
    
    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("app_name", "app_nme"), 1);
        assert_eq!(levenshtein_distance("config", "config"), 0);
        assert_eq!(levenshtein_distance("test", ""), 4);
        assert_eq!(levenshtein_distance("", "test"), 4);
    }
}
