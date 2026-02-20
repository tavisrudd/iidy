use crate::yaml::errors::display;
use crate::yaml::errors::ErrorId;
use std::fmt;

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

    CloudFormationValidation {
        error_id: ErrorId,
        tag_name: String,
        message: String,
        location: SourceLocation,
        help_text: String,
    },

    YamlSyntax {
        error_id: ErrorId,
        short_message: String,
        guidance: String,
        location: SourceLocation,
        fix_hint: Option<String>,
        example: Option<String>,
    },

    TagParsing {
        error_id: ErrorId,
        tag_name: String,
        message: String,
        location: SourceLocation,
        suggestion: Option<String>,
        caret_column: usize,
        span_len: usize,
    },

    LookupQuery {
        error_id: ErrorId,
        variable_path: String,
        message: String,
        location: SourceLocation,
        available_keys: Vec<String>,
    },
}

impl EnhancedPreprocessingError {
    pub fn error_id(&self) -> ErrorId {
        match self {
            Self::VariableNotFound { error_id, .. }
            | Self::TypeMismatch { error_id, .. }
            | Self::CloudFormationValidation { error_id, .. }
            | Self::YamlSyntax { error_id, .. }
            | Self::TagParsing { error_id, .. }
            | Self::LookupQuery { error_id, .. } => *error_id,
        }
    }

    pub fn location(&self) -> &SourceLocation {
        match self {
            Self::VariableNotFound { location, .. }
            | Self::TypeMismatch { location, .. }
            | Self::CloudFormationValidation { location, .. }
            | Self::YamlSyntax { location, .. }
            | Self::TagParsing { location, .. }
            | Self::LookupQuery { location, .. } => location,
        }
    }

    /// Display error with clean, user-friendly formatting
    pub fn display_with_context(&self, source_lines: Option<&[String]>) -> String {
        // New variants have their own rendering to match existing output formats
        match self {
            Self::YamlSyntax { .. } => return self.render_yaml_syntax(source_lines),
            Self::TagParsing { .. } => return self.render_tag_parsing(source_lines),
            Self::LookupQuery { .. } => return self.render_lookup_query(source_lines),
            _ => {}
        }

        let mut output = String::new();
        let loc = self.location();
        let c = display::ErrorColors::detect();

        let error_type = match self {
            Self::VariableNotFound { .. } => "Variable error",
            Self::TypeMismatch { .. } => "Type error",
            Self::CloudFormationValidation { .. } => "CloudFormation error",
            Self::YamlSyntax { .. } | Self::TagParsing { .. } | Self::LookupQuery { .. } => {
                unreachable!()
            }
        };

        let short_message = match self {
            Self::VariableNotFound { variable, .. } => format!("'{}' not found", variable),
            Self::TypeMismatch {
                expected, found, ..
            } => format!("expected {}, found {}", expected, found),
            Self::CloudFormationValidation { message, .. } => message.clone(),
            Self::YamlSyntax { .. } | Self::TagParsing { .. } | Self::LookupQuery { .. } => {
                unreachable!()
            }
        };

        output.push_str(&format!(
            "{}{}{}: {} @ {}{}{} {}(errno: {}){}\n",
            c.bold_red,
            error_type,
            c.reset,
            short_message,
            c.cyan,
            loc,
            c.reset,
            c.grey,
            self.error_id().code(),
            c.reset
        ));

        let guidance = match self {
            Self::VariableNotFound { .. } => "variable not defined in current scope",
            Self::TypeMismatch { .. } => "data type mismatch",
            Self::CloudFormationValidation { .. } => "invalid CloudFormation intrinsic function",
            Self::YamlSyntax { .. } | Self::TagParsing { .. } | Self::LookupQuery { .. } => {
                unreachable!()
            }
        };

        output.push_str(&format!("{}  -> {}{}\n", c.light_blue, guidance, c.reset));

        if let Some(lines) = source_lines {
            let ctx = display::format_source_context(
                lines,
                loc.line,
                loc.column,
                self.error_span_length(),
                &self.inline_description(),
                &c,
            );
            if !ctx.is_empty() {
                output.push('\n');
                output.push_str(&ctx);
                output.push('\n');
            }
        }

        match self {
            Self::VariableNotFound {
                suggestions,
                available_vars,
                ..
            } => {
                if !suggestions.is_empty() {
                    output.push_str(&format!(
                        "{}   did you mean '{}'?{}\n",
                        c.light_blue,
                        suggestions.join("' or '"),
                        c.reset
                    ));
                }
                if !available_vars.is_empty() {
                    let mut sorted_vars = available_vars.clone();
                    sorted_vars.sort();
                    output.push_str(&format!(
                        "{}   available variables: {}{}\n",
                        c.light_blue,
                        sorted_vars.join(", "),
                        c.reset
                    ));
                }
            }
            Self::TypeMismatch {
                expected,
                found,
                help,
                ..
            } => {
                output.push_str(&format!(
                    "{}   expected {}, found {}{}\n",
                    c.light_blue, expected, found, c.reset
                ));
                if let Some(type_help) = help {
                    output.push_str(&format!("{}   {}{}\n", c.light_blue, type_help, c.reset));
                }
            }
            Self::CloudFormationValidation { help_text, .. } => {
                output.push_str(&format!("{}   {}{}\n", c.light_blue, help_text, c.reset));
                let help_messages = self.help_messages();
                for help in &help_messages[1..] {
                    if help.starts_with("example:") {
                        output.push_str(&format!("{}   {}{}\n", c.light_blue, help, c.reset));
                    }
                }
            }
            _ => {
                let help_messages = self.help_messages();
                if let Some(help) = help_messages.first() {
                    output.push_str(&format!("{}   {}{}\n", c.light_blue, help, c.reset));
                }
            }
        }

        output.push_str(&format!(
            "\n{}   For more info: iidy explain {}{}\n",
            c.light_blue,
            self.error_id().code(),
            c.reset
        ));

        output
    }

    fn render_yaml_syntax(&self, source_lines: Option<&[String]>) -> String {
        let Self::YamlSyntax {
            short_message,
            guidance,
            location,
            fix_hint,
            example,
            ..
        } = self
        else {
            unreachable!()
        };

        let c = display::ErrorColors::detect();
        let mut output = format!(
            "{}Syntax error{}: {} @ {}{}{} {}(errno: {}){}\n",
            c.bold_red,
            c.reset,
            short_message,
            c.cyan,
            location,
            c.reset,
            c.grey,
            self.error_id().code(),
            c.reset
        );
        output.push_str(&format!("{}  -> {}{}\n", c.light_blue, guidance, c.reset));

        if let Some(lines) = source_lines {
            let ctx = display::format_source_context(
                lines,
                location.line,
                location.column,
                1,
                "",
                &c,
            );
            if !ctx.is_empty() {
                output.push('\n');
                output.push_str(&ctx);

                if let Some(fix) = fix_hint {
                    output.push_str(&format!(
                        "\n{}   fix: {}{}\n",
                        c.light_blue, fix, c.reset
                    ));
                }
                if let Some(ex) = example {
                    output.push_str(&format!(
                        "{}   example: {}{}\n",
                        c.light_blue, ex, c.reset
                    ));
                }

                output.push_str(&format!(
                    "\n{}   For more info: iidy explain {}{}\n",
                    c.light_blue,
                    self.error_id().code(),
                    c.reset
                ));
            } else {
                output.push_str(&format!(
                    "\n{}   For more info: iidy explain {}{}\n",
                    c.light_blue,
                    self.error_id().code(),
                    c.reset
                ));
            }
        } else {
            output.push_str(&format!(
                "\n{}   For more info: iidy explain {}{}\n",
                c.light_blue,
                self.error_id().code(),
                c.reset
            ));
        }

        output
    }

    fn render_tag_parsing(&self, source_lines: Option<&[String]>) -> String {
        let Self::TagParsing {
            tag_name,
            message,
            location,
            suggestion,
            caret_column,
            span_len,
            ..
        } = self
        else {
            unreachable!()
        };

        let c = display::ErrorColors::detect();
        let mut output = format!(
            "{}Tag error{}: {} @ {}{}{} {}(errno: {}){}\n",
            c.bold_red,
            c.reset,
            message,
            c.cyan,
            location,
            c.reset,
            c.grey,
            self.error_id().code(),
            c.reset
        );

        let guidance_text = suggestion.as_deref().unwrap_or("invalid tag or syntax");
        output.push_str(&format!("{}  -> {}{}\n", c.light_blue, guidance_text, c.reset));

        // Source context (always preceded by blank line)
        output.push('\n');
        if let Some(lines) = source_lines {
            let ctx = display::format_source_context(
                lines,
                location.line,
                *caret_column,
                *span_len,
                "",
                &c,
            );
            output.push_str(&ctx);
        }

        // Tag-specific example
        output.push_str(&display::tag_example(tag_name, &c));

        output.push_str(&format!(
            "{}   For more info: iidy explain {}{}\n",
            c.light_blue,
            self.error_id().code(),
            c.reset
        ));

        output
    }

    fn render_lookup_query(&self, source_lines: Option<&[String]>) -> String {
        let Self::LookupQuery {
            variable_path,
            message,
            location,
            available_keys,
            ..
        } = self
        else {
            unreachable!()
        };

        let c = display::ErrorColors::detect();
        let mut output = format!(
            "{}Lookup error{}: {} @ {}{}{} {}(errno: {}){}\n",
            c.bold_red,
            c.reset,
            message,
            c.cyan,
            location,
            c.reset,
            c.grey,
            self.error_id().code(),
            c.reset
        );
        output.push_str(&format!(
            "{}  -> query failed on variable '{}'{}\n",
            c.light_blue, variable_path, c.reset
        ));

        if let Some(lines) = source_lines {
            if location.line > 0 {
                output.push('\n');
                let ctx =
                    display::format_source_context(lines, location.line, 0, 0, "", &c);
                output.push_str(&ctx);
                output.push('\n');
            }
        }

        if !available_keys.is_empty() {
            let mut sorted = available_keys.clone();
            sorted.sort();
            output.push_str(&format!(
                "{}   available keys: {}{}\n",
                c.light_blue,
                sorted.join(", "),
                c.reset
            ));
        }

        output.push_str(&format!(
            "\n{}   For more info: iidy explain {}{}\n",
            c.light_blue,
            self.error_id().code(),
            c.reset
        ));

        output
    }

    fn inline_description(&self) -> String {
        match self {
            Self::VariableNotFound { .. } => "variable not defined".to_string(),
            Self::TypeMismatch { expected, .. } => format!("expected {}", expected),
            Self::CloudFormationValidation { .. } => "invalid CloudFormation tag".to_string(),
            // New variants use their own render methods, these aren't called
            Self::YamlSyntax { .. } => String::new(),
            Self::TagParsing { .. } => String::new(),
            Self::LookupQuery { .. } => String::new(),
        }
    }

    fn error_span_length(&self) -> usize {
        match self {
            Self::VariableNotFound { variable, .. } => variable.len() + 3, // "!$ " prefix
            Self::CloudFormationValidation { .. } => 4, // Reasonable span for values like "null", "[]"
            _ => 8,                                     // Default span length
        }
    }

    fn help_messages(&self) -> Vec<String> {
        let mut help = Vec::new();

        match self {
            Self::VariableNotFound {
                available_vars,
                suggestions,
                ..
            } => {
                if !available_vars.is_empty() {
                    let mut sorted_vars = available_vars.clone();
                    sorted_vars.sort();
                    help.push(format!(
                        "available variables in this scope: {}",
                        sorted_vars.join(", ")
                    ));
                }
                for suggestion in suggestions {
                    help.push(format!("did you mean '{}'?", suggestion));
                }
            }
            Self::TypeMismatch {
                help: type_help, ..
            } => {
                if let Some(h) = type_help {
                    help.push(h.clone());
                }
            }
            Self::CloudFormationValidation {
                help_text,
                tag_name,
                ..
            } => {
                help.push(help_text.clone());

                // Add examples based on tag type
                match tag_name.as_str() {
                    "Ref" => {
                        help.push("example: BucketName: !Ref MyBucket".to_string());
                        help.push("example: Environment: !Ref EnvironmentParam".to_string());
                    }
                    "Sub" => {
                        help.push("example: UserData: !Sub 'Hello ${AWS::StackName}'".to_string());
                        help.push(
                            "example: Message: !Sub ['Hello ${name}', {name: MyValue}]".to_string(),
                        );
                    }
                    "GetAtt" => {
                        help.push("example: DnsName: !GetAtt LoadBalancer.DNSName".to_string());
                        help.push("example: Value: !GetAtt [MyResource, Attribute]".to_string());
                    }
                    "Join" => {
                        help.push(
                            "example: Name: !Join ['-', [!Ref 'AWS::StackName', 'suffix']]"
                                .to_string(),
                        );
                    }
                    "Select" => {
                        help.push("example: AZ: !Select [0, !GetAZs '']".to_string());
                    }
                    "Split" => {
                        help.push("example: Parts: !Split [',', 'a,b,c']".to_string());
                    }
                    "FindInMap" => {
                        help.push(
                            "example: AMI: !FindInMap [RegionMap, !Ref 'AWS::Region', AMI]"
                                .to_string(),
                        );
                    }
                    "Base64" => {
                        help.push("example: UserData: !Base64 'echo Hello'".to_string());
                        help.push("example: Script: !Base64 !Sub 'echo ${Parameter}'".to_string());
                    }
                    _ => {}
                }
            }
            // New variants use their own render methods
            Self::YamlSyntax { .. }
            | Self::TagParsing { .. }
            | Self::LookupQuery { .. } => {}
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

    pub fn cloudformation_validation(
        tag_name: &str,
        message: &str,
        location: SourceLocation,
    ) -> Self {
        // Generate tag-specific help text
        let help_text = match tag_name {
            "Ref" => "!Ref expects a string (resource or parameter name)",
            "Sub" => "!Sub expects a string or [string, variables] array",
            "GetAtt" => "!GetAtt expects 'Resource.Attribute' or [Resource, Attribute]",
            "Join" => "!Join expects [delimiter, array] with exactly 2 elements",
            "Select" => "!Select expects [index, array] with exactly 2 elements",
            "Split" => "!Split expects [delimiter, string] with exactly 2 elements",
            "FindInMap" => "!FindInMap expects [MapName, TopLevelKey, SecondLevelKey]",
            "Base64" => "!Base64 expects a non-null string value",
            _ => "check CloudFormation documentation for proper syntax",
        };

        Self::CloudFormationValidation {
            error_id: ErrorId::InvalidCloudFormationIntrinsic,
            tag_name: tag_name.to_string(),
            message: message.to_string(),
            location,
            help_text: help_text.to_string(),
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

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut matrix = vec![vec![0; n + 1]; m + 1];

    for i in 0..=m {
        matrix[i][0] = i;
    }
    for j in 0..=n {
        matrix[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[m][n]
}

/// Generate helpful suggestions for type conversion
fn generate_type_conversion_help(expected: &str, found: &str) -> Option<String> {
    match (expected, found) {
        ("array", "string") => {
            Some("try using !$split to convert a string to an array".to_string())
        }
        ("string", "array") => Some("try using !$join to convert an array to a string".to_string()),
        ("object", "string") => {
            Some("try using !$parseJson or !$parseYaml to parse the string".to_string())
        }
        ("string", "object") => {
            Some("try using !$toJsonString or !$toYamlString to serialize the object".to_string())
        }
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
        assert!(display.contains("ERR_2001"));
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
