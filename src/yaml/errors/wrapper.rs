use crate::yaml::errors::display;
use crate::yaml::errors::{EnhancedPreprocessingError, ErrorId, SourceLocation};

/// Wraps an EnhancedPreprocessingError with source context for display through anyhow.
/// Stores the structured error alongside source lines so rendering happens at display time.
#[derive(Debug)]
pub struct FormattedError {
    inner: EnhancedPreprocessingError,
    source_lines: Option<Vec<String>>,
}

impl FormattedError {
    fn new(error: EnhancedPreprocessingError, source_lines: Option<Vec<String>>) -> Self {
        Self {
            inner: error,
            source_lines,
        }
    }
}

impl std::fmt::Display for FormattedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.inner.display_with_context(self.source_lines.as_deref())
        )
    }
}

impl std::error::Error for FormattedError {}

/// Wrapper for variable not found errors
pub fn variable_not_found_error(
    variable: &str,
    file_path: &str,
    yaml_path: &str,
    available_vars: Vec<String>,
) -> anyhow::Error {
    let (actual_file_path, provided_line_number) = display::parse_file_location(file_path);

    let (source_lines, line_number, column_number) =
        if let Ok(content) = std::fs::read_to_string(actual_file_path) {
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

            if let Some(line_num) = provided_line_number {
                let column_num = if line_num > 0 && line_num <= lines.len() {
                    let line_content = &lines[line_num - 1];
                    if let Some(col) = line_content.find(&format!("{{{{{}}}}}", variable)) {
                        col + 2
                    } else if let Some(col) = line_content.find(&format!("!$ {}", variable)) {
                        col + 4
                    } else if let Some(col) = line_content.find(&format!("!${}", variable)) {
                        col + 3
                    } else {
                        0
                    }
                } else {
                    0
                };
                (Some(lines), line_num, column_num)
            } else {
                let (line_num, column_num) = lines
                    .iter()
                    .enumerate()
                    .find_map(|(idx, line)| {
                        if let Some(col) = line.find(&format!("!$ {}", variable)) {
                            Some((idx + 1, col + 4))
                        } else if let Some(col) = line.find(&format!("!${}", variable)) {
                            Some((idx + 1, col + 3))
                        } else if let Some(col) = line.find(&format!("{{{{{}}}}}", variable)) {
                            Some((idx + 1, col + 2))
                        } else {
                            None
                        }
                    })
                    .unwrap_or((0, 0));

                (Some(lines), line_num, column_num)
            }
        } else {
            (None, 0, 0)
        };

    let location = SourceLocation::new(actual_file_path, line_number, column_number, yaml_path);
    let error =
        EnhancedPreprocessingError::variable_not_found(variable, location, available_vars);

    anyhow::Error::new(FormattedError::new(error, source_lines))
}

/// Wrapper for missing required field errors - delegates to tag_parsing_error for consistency
pub fn missing_required_field_error(
    tag_name: &str,
    missing_field: &str,
    file_path: &str,
    _yaml_path: &str,
    _required_fields: Vec<String>,
) -> anyhow::Error {
    let message = format!("'{}' missing in {} tag", missing_field, tag_name);
    let suggestion = format!("add '{}' field to {} tag", missing_field, tag_name);
    tag_parsing_error(tag_name, &message, file_path, Some(&suggestion))
}

/// Wrapper for YAML syntax errors
pub fn yaml_syntax_error(
    yaml_error: serde_yaml::Error,
    file_path: &str,
    input: &str,
) -> anyhow::Error {
    let location_info = yaml_error.location().map(|loc| (loc.line(), loc.column()));
    let error_msg = yaml_error.to_string();

    let short_message = if error_msg.contains("did not find expected key") {
        "invalid YAML structure"
    } else if error_msg.contains("while parsing a block mapping") {
        "invalid block mapping"
    } else if error_msg.contains("found unexpected end of stream") {
        "unexpected end of file"
    } else if error_msg.contains("expected") {
        "syntax error"
    } else {
        "parsing error"
    };

    let guidance = if error_msg.contains("did not find expected key") {
        "tags cannot be chained - use list syntax"
    } else if error_msg.contains("while parsing a block mapping") {
        "check indentation and YAML structure"
    } else if error_msg.contains("found unexpected end of stream") {
        "missing closing quote or bracket"
    } else {
        "check YAML syntax"
    };

    let (line_num, col_num) = location_info.unwrap_or((0, 0));
    let source_lines: Vec<String> = input.lines().map(|s| s.to_string()).collect();

    let fix_hint = if line_num > 0 && line_num <= source_lines.len() {
        let line_content = &source_lines[line_num - 1];
        if error_msg.contains("did not find expected key") && line_content.contains("!$") {
            Some("put the inner tag in a list to separate it from the outer tag".to_string())
        } else {
            None
        }
    } else {
        None
    };
    let example = if fix_hint.is_some() {
        Some("!$not [!$eq [\"a\", \"b\"]]".to_string())
    } else {
        None
    };

    let location = SourceLocation::new(file_path, line_num, col_num, "");
    let error = EnhancedPreprocessingError::YamlSyntax {
        error_id: ErrorId::InvalidYamlSyntax,
        short_message: short_message.to_string(),
        guidance: guidance.to_string(),
        location,
        fix_hint,
        example,
    };

    anyhow::Error::new(FormattedError::new(error, Some(source_lines)))
}

/// Wrapper for tag parsing errors with automatic example generation
pub fn tag_parsing_error(
    tag_name: &str,
    message: &str,
    file_path: &str,
    suggestion: Option<&str>,
) -> anyhow::Error {
    let (actual_file_path, line_number, parser_column) =
        display::parse_file_location_full(file_path);

    let (source_lines, caret_column, span_len) = if let Some(line_num) = line_number {
        if let Ok(content) = std::fs::read_to_string(actual_file_path) {
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
            let (col, span) = if line_num > 0 && line_num <= lines.len() {
                display::tag_error_caret(&lines[line_num - 1], message)
            } else {
                (0, 0)
            };
            (Some(lines), col, span)
        } else {
            (None, 0, 0)
        }
    } else {
        (None, 0, 0)
    };

    let location = SourceLocation::new(
        actual_file_path,
        line_number.unwrap_or(0),
        parser_column.unwrap_or(0),
        "",
    );
    let error = EnhancedPreprocessingError::TagParsing {
        error_id: ErrorId::MissingRequiredTagField,
        tag_name: tag_name.to_string(),
        message: message.to_string(),
        location,
        suggestion: suggestion.map(|s| s.to_string()),
        caret_column,
        span_len,
    };

    anyhow::Error::new(FormattedError::new(error, source_lines))
}


/// Wrapper for variable not found errors with PathTracker support
pub fn variable_not_found_error_with_path_tracker(
    variable: &str,
    file_path: &str,
    path: &crate::yaml::path_tracker::PathTracker,
    available_vars: Vec<String>,
) -> anyhow::Error {
    let yaml_path = path.current_path();
    variable_not_found_error(variable, file_path, &yaml_path, available_vars)
}

/// Wrapper for type mismatch errors
pub fn type_mismatch_error_with_path_tracker(
    expected_type: &str,
    found_type: &str,
    context_description: &str,
    file_path: &str,
    path: &crate::yaml::path_tracker::PathTracker,
) -> anyhow::Error {
    let yaml_path = path.current_path();
    type_mismatch_error_impl(
        expected_type,
        found_type,
        context_description,
        file_path,
        &yaml_path,
    )
}

/// Internal implementation for type mismatch errors
fn type_mismatch_error_impl(
    expected_type: &str,
    found_type: &str,
    context_description: &str,
    file_path: &str,
    yaml_path: &str,
) -> anyhow::Error {
    let (actual_file_path, provided_line_number) = display::parse_file_location(file_path);

    let (source_lines, line_number, column_number) = if let Ok(content) =
        std::fs::read_to_string(actual_file_path)
    {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        if let Some(line_num) = provided_line_number {
            let column_num = if line_num > 0 && line_num <= lines.len() {
                display::find_tag_column(&lines[line_num - 1], context_description)
            } else {
                0
            };
            (Some(lines), line_num, column_num)
        } else {
            // Search for the tag that likely caused the error, ignoring comments
            let (line_num, column_num) = lines
                .iter()
                .enumerate()
                .find_map(|(idx, line)| {
                    let trimmed_line = line.trim_start();
                    if trimmed_line.starts_with('#') {
                        return None;
                    }

                    if context_description.contains("!$split") && line.contains("!$split") {
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else if context_description.contains("!$join") && line.contains("!$join") {
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else if context_description.contains("!$groupBy")
                        && line.contains("!$groupBy")
                    {
                        if context_description.contains("items field") {
                            if let Some(result) = display::search_field_on_subsequent_lines(
                                &lines,
                                idx,
                                "items:",
                                context_description,
                            ) {
                                return Some(result);
                            }
                        }
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else if context_description.contains("!$mapListToHash")
                        && line.contains("!$mapListToHash")
                    {
                        if context_description.contains("items field") {
                            if let Some(result) = display::search_field_on_subsequent_lines(
                                &lines,
                                idx,
                                "items:",
                                context_description,
                            ) {
                                return Some(result);
                            }
                        }
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else if context_description.contains("!$fromPairs")
                        && line.contains("!$fromPairs")
                    {
                        if context_description.contains("source field") {
                            if let Some(result) = display::search_field_on_subsequent_lines(
                                &lines,
                                idx,
                                "source:",
                                context_description,
                            ) {
                                return Some(result);
                            }
                        }
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else if context_description.contains("!$map") && line.contains("!$map") {
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else if context_description.contains("!$merge") && line.contains("!$merge") {
                        Some((idx + 1, display::find_tag_column(line, context_description)))
                    } else {
                        None
                    }
                })
                .unwrap_or((0, 0));

            (Some(lines), line_num, column_num)
        }
    } else {
        (None, 0, 0)
    };

    let location = SourceLocation::new(actual_file_path, line_number, column_number, yaml_path);
    let error = EnhancedPreprocessingError::type_mismatch(
        expected_type,
        found_type,
        location,
        context_description,
    );

    anyhow::Error::new(FormattedError::new(error, source_lines))
}

/// Enhanced CloudFormation validation error
pub fn cloudformation_validation_error_with_path_tracker(
    tag_name: &str,
    message: &str,
    file_path: &str,
    path: &crate::yaml::path_tracker::PathTracker,
) -> anyhow::Error {
    let yaml_path = path.current_path();
    cloudformation_validation_error_impl(tag_name, message, file_path, &yaml_path)
}

/// Internal implementation for CloudFormation validation errors
fn cloudformation_validation_error_impl(
    tag_name: &str,
    message: &str,
    file_path: &str,
    yaml_path: &str,
) -> anyhow::Error {
    let (actual_file_path, provided_line_number) = display::parse_file_location(file_path);

    let (source_lines, line_number, column_number) =
        if let Ok(content) = std::fs::read_to_string(actual_file_path) {
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

            // Always search for the actual CloudFormation tag location, ignoring provided line number
            // because it's often inaccurate for CloudFormation tags
            let mut found_line = 0;
            let mut found_col = 1;

            for (idx, line) in lines.iter().enumerate() {
                let trimmed_line = line.trim_start();
                if !trimmed_line.starts_with('#') {
                    if let Some(tag_col) = line.find(&format!("!{}", tag_name)) {
                        let tag_end = tag_col + tag_name.len() + 1;
                        if tag_end < line.len() {
                            let next_char = line.chars().nth(tag_end);
                            if matches!(next_char, Some(' ') | Some('[') | Some('\t')) {
                                found_line = idx + 1;
                                let after_tag = &line[tag_end..];
                                let value_start =
                                    after_tag.chars().take_while(|c| c.is_whitespace()).count();
                                found_col = tag_end + value_start + 1;
                                break;
                            }
                        }
                    }
                }
            }

            if found_line > 0 {
                (Some(lines), found_line, found_col)
            } else {
                let fallback_line = provided_line_number.unwrap_or(1);
                (Some(lines), fallback_line, 1)
            }
        } else {
            (None, 1, 1)
        };

    let location = SourceLocation::new(actual_file_path, line_number, column_number, yaml_path);
    let error = EnhancedPreprocessingError::cloudformation_validation(tag_name, message, location);

    anyhow::Error::new(FormattedError::new(error, source_lines))
}

/// Error for query/JMESPath failures on a resolved variable lookup.
pub fn lookup_query_error(
    variable_path: &str,
    message: &str,
    file_path: &str,
    line_number: usize,
    available_keys: &[String],
) -> anyhow::Error {
    let source_lines = if line_number > 0 {
        std::fs::read_to_string(file_path)
            .ok()
            .map(|content| content.lines().map(|s| s.to_string()).collect::<Vec<_>>())
    } else {
        None
    };

    let location = SourceLocation::new(file_path, line_number, 0, "");
    let error = EnhancedPreprocessingError::LookupQuery {
        error_id: ErrorId::VariableNotFound,
        variable_path: variable_path.to_string(),
        message: message.to_string(),
        location,
        available_keys: available_keys.to_vec(),
    };

    anyhow::Error::new(FormattedError::new(error, source_lines))
}
