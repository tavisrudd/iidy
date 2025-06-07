use std::fmt;
use crate::yaml::error_ids::ErrorId;

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
    
    /// Display error with Rust-style formatting including error ID
    pub fn display_with_context(&self, source_lines: Option<&[String]>) -> String {
        let mut output = String::new();
        
        // Error header with ID
        output.push_str(&format!("error[{}]: {}\n", 
            self.error_id().code(), 
            self.main_message()
        ));
        
        // Source location
        let loc = self.location();
        if loc.line > 0 {
            output.push_str(&format!("  --> {}\n", loc));
        }
        
        // Source context if available
        if let Some(lines) = source_lines {
            if loc.line > 0 && loc.line <= lines.len() {
                output.push_str("   |\n");
                
                // Show the problematic line
                let line_content = &lines[loc.line - 1];
                output.push_str(&format!("{:4} | {}\n", loc.line, line_content));
                
                // Show caret pointing to the error
                if loc.column > 0 {
                    let spaces = " ".repeat(loc.column - 1);
                    let carets = "^".repeat(self.error_span_length().min(line_content.len() - loc.column + 1));
                    output.push_str(&format!("   | {}{} {}\n", spaces, carets, self.inline_description()));
                }
                
                output.push_str("   |\n");
            }
        }
        
        // Help and suggestions
        for note in self.notes() {
            output.push_str(&format!("   = note: {}\n", note));
        }
        
        for help in self.help_messages() {
            output.push_str(&format!("   = help: {}\n", help));
        }
        
        // Reference to detailed help
        output.push_str(&format!("   = help: for more information about this error, try `iidy explain {}`\n", 
            self.error_id().code()));
        
        output
    }
    
    fn main_message(&self) -> String {
        match self {
            Self::VariableNotFound { variable, .. } => 
                format!("variable '{}' not found", variable),
            Self::TypeMismatch { expected, found, .. } => 
                format!("type mismatch: expected {}, found {}", expected, found),
            Self::MissingRequiredField { tag_name, missing_field, .. } => 
                format!("missing required field '{}' in {} tag", missing_field, tag_name),
            Self::ImportError { import_type, location_str, .. } => 
                format!("failed to load {} import: {}", import_type, location_str),
            Self::HandlebarsError { template, .. } => 
                format!("handlebars template error in: {}", template),
        }
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
    
    fn notes(&self) -> Vec<String> {
        match self {
            Self::VariableNotFound { .. } => vec![
                "only variables from $defs, $imports, and local scoped variables are available".to_string()
            ],
            Self::TypeMismatch { context, .. } => vec![
                format!("this operation requires compatible data types"),
                format!("in context: {}", context)
            ],
            Self::MissingRequiredField { tag_name, required_fields, .. } => vec![
                format!("{} tag requires the following fields: {}", tag_name, required_fields.join(", "))
            ],
            Self::ImportError { .. } => vec![
                "import paths are resolved relative to the current file".to_string()
            ],
            Self::HandlebarsError { .. } => vec![
                "handlebars templates use {{variable}} syntax".to_string()
            ],
        }
    }
    
    fn help_messages(&self) -> Vec<String> {
        let mut help = Vec::new();
        
        match self {
            Self::VariableNotFound { available_vars, suggestions, .. } => {
                if !available_vars.is_empty() {
                    help.push(format!("available variables in this scope: {}", available_vars.join(", ")));
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
                    "condition" => help.push("example: condition: true".to_string()),
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
        assert!(display.contains("error[IY2001]"));
        assert!(display.contains("variable 'app_nme' not found"));
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