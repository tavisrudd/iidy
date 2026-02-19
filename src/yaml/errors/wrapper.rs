use crate::yaml::errors::{EnhancedPreprocessingError, SourceLocation};

// TODO: ERROR SAFETY AUDIT - This module contains multiple panic potential scenarios that need fixing:
// 1. Array index out of bounds: lines[line_num - 1] without bounds checking
// 2. String slicing panics: line_content[pos + offset..] without length validation
// 3. Character index access: line.chars().nth() with potentially invalid indices
// 4. Arithmetic overflow: position calculations on very large files
// 5. File path parsing: parts[1] access without bounds checking
// 6. Display generation failures: error.display_with_context() could panic during error generation
// All error constructors should use safe operations and graceful degradation to ensure
// they never panic during error generation, as that would mask the original error.

/// Custom error type that implements the marker trait
#[derive(Debug)]
pub struct EnhancedErrorWrapper {
    pub message: String,
}

impl std::fmt::Display for EnhancedErrorWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EnhancedErrorWrapper {}

/// Wrapper for variable not found errors that switches between basic and enhanced error reporting
#[allow(unused_variables)]
pub fn variable_not_found_error(
    variable: &str,
    file_path: &str,
    yaml_path: &str,
    available_vars: Vec<String>,
) -> anyhow::Error {
    {
        // Enhanced error format with error IDs and suggestions
        // Parse line number from file_path if present (e.g., "file.yaml:6")
        let (actual_file_path, provided_line_number) = if file_path.contains(':') {
            let parts: Vec<&str> = file_path.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(line_num) = parts[1].parse::<usize>() {
                    // TODO: PANIC POTENTIAL - parts[1] access without bounds check
                    (parts[0], Some(line_num))
                } else {
                    (file_path, None)
                }
            } else {
                (file_path, None)
            }
        } else {
            (file_path, None)
        };

        // Try to read the source file for context and find the line
        let (source_lines, line_number, column_number) =
            if let Ok(content) = std::fs::read_to_string(actual_file_path) {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

                // If we already have a line number from the caller, use it and find the column
                if let Some(line_num) = provided_line_number {
                    let column_num = if line_num > 0 && line_num <= lines.len() {
                        let line_content = &lines[line_num - 1]; // TODO: PANIC POTENTIAL - line_num could be 0 causing underflow
                        if let Some(col) = line_content.find(&format!("{{{{{}}}}}", variable)) {
                            col + 2 // +2 to point after "{{"
                        } else if let Some(col) = line_content.find(&format!("!$ {}", variable)) {
                            col + 4 // +4 to point after "!$ "
                        } else if let Some(col) = line_content.find(&format!("!${}", variable)) {
                            col + 3 // +3 to point after "!$"
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    (Some(lines), line_num, column_num)
                } else {
                    // Simple heuristic: find line containing the variable reference
                    let (line_num, column_num) = lines
                        .iter()
                        .enumerate()
                        .find_map(|(idx, line)| {
                            if let Some(col) = line.find(&format!("!$ {}", variable)) {
                                Some((idx + 1, col + 4)) // +4 to point after "!$ "
                            } else if let Some(col) = line.find(&format!("!${}", variable)) {
                                Some((idx + 1, col + 3)) // +3 to point after "!$"
                            } else if let Some(col) = line.find(&format!("{{{{{}}}}}", variable)) {
                                Some((idx + 1, col + 2)) // +2 to point after "{{"
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

        let enhanced_display = error.display_with_context(source_lines.as_deref()); // TODO: PANIC POTENTIAL - display generation could panic during error creation
        anyhow::Error::new(EnhancedErrorWrapper {
            message: enhanced_display,
        })
    }
}

/// Wrapper for missing required field errors - now uses tag_parsing_error for consistency
#[allow(unused_variables)]
pub fn missing_required_field_error(
    tag_name: &str,
    missing_field: &str,
    file_path: &str,
    yaml_path: &str,
    required_fields: Vec<String>,
) -> anyhow::Error {
    let message = format!("'{}' missing in {} tag", missing_field, tag_name);
    let suggestion = format!("add '{}' field to {} tag", missing_field, tag_name);

    // Use the consistent tag_parsing_error function so we get examples
    tag_parsing_error(tag_name, &message, file_path, Some(&suggestion))
}

/// Wrapper for YAML syntax errors
#[allow(unused_variables)]
pub fn yaml_syntax_error(
    yaml_error: serde_yaml::Error,
    file_path: &str,
    input: &str,
) -> anyhow::Error {
    {
        // Enhanced error format with better location and suggestions
        let location_info = yaml_error.location().map(|loc| (loc.line(), loc.column()));
        let error_msg = yaml_error.to_string();

        // Try to provide better error messages for common issues
        let short_msg = if error_msg.contains("did not find expected key") {
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

        let source_lines = input.lines().map(|s| s.to_string()).collect::<Vec<_>>();
        let location = SourceLocation::new(file_path, line_num, col_num, "<root>");
        let use_color = std::env::var("NO_COLOR").is_err() && atty::is(atty::Stream::Stderr);

        // Color codes
        let bold_red = if use_color { "\x1b[1;31m" } else { "" };
        let red = if use_color { "\x1b[31m" } else { "" };
        let _yellow = if use_color { "\x1b[33m" } else { "" };
        let cyan = if use_color { "\x1b[36m" } else { "" };
        let blue_grey = if use_color { "\x1b[38;5;245m" } else { "" }; // lighter grey for source context
        let light_blue = if use_color { "\x1b[38;5;75m" } else { "" }; // light blue for help text
        let grey = if use_color { "\x1b[90m" } else { "" }; // grey for line numbers
        let reset = if use_color { "\x1b[0m" } else { "" };

        // Create a concise parsing error header
        let error_display = format!(
            "{}Syntax error{}: {} @ {}{}:{}:{}{} {}(errno: ERR_1001){}\n",
            bold_red, reset, short_msg, cyan, file_path, line_num, col_num, reset, grey, reset
        );

        // Add guidance line
        let guidance_line = format!("{}  -> {}{}\n", light_blue, guidance, reset);

        // Add source context if we have valid line numbers
        let final_display = if line_num > 0 && line_num <= source_lines.len() {
            let mut output = error_display;
            output.push_str(&guidance_line);
            output.push_str("\n");

            // Show line before for context (if available) - in blue-grey with grey line number
            if line_num > 1 {
                let prev_line = &source_lines[line_num - 2];
                output.push_str(&format!(
                    "{}{:4}{} | {}{}{}\n",
                    grey,
                    line_num - 1,
                    reset,
                    blue_grey,
                    prev_line,
                    reset
                ));
            }

            // Show the problematic line - make line number red to draw attention
            let line_content = &source_lines[line_num - 1];
            output.push_str(&format!(
                "{}{:4}{} | {}\n",
                red, line_num, reset, line_content
            ));

            // Show caret if we have column info
            if col_num > 0 && col_num <= line_content.len() {
                let spaces = " ".repeat(col_num - 1); // column offset
                output.push_str(&format!("     | {}{}^{}\n", spaces, red, reset));
            }

            // Show line after for context (if available) - in blue-grey with grey line number
            if line_num < source_lines.len() {
                let next_line = &source_lines[line_num];
                output.push_str(&format!(
                    "{}{:4}{} | {}{}{}\n",
                    grey,
                    line_num + 1,
                    reset,
                    blue_grey,
                    next_line,
                    reset
                ));
            }

            // Add specific help for common issues
            if error_msg.contains("did not find expected key") && line_content.contains("!$") {
                output.push_str(&format!(
                    "\n{}   fix: put the inner tag in a list to separate it from the outer tag{}\n",
                    light_blue, reset
                ));
                output.push_str(&format!(
                    "{}   example: !$not [!$eq [\"a\", \"b\"]]{}\n",
                    light_blue, reset
                ));
            }

            output.push_str(&format!(
                "\n{}   For more info, run: iidy explain ERR_1001{}\n",
                light_blue, reset
            ));
            output
        } else {
            format!(
                "{}{}\n{}   For more info, run: iidy explain ERR_1001{}\n",
                error_display, guidance_line, light_blue, reset
            )
        };

        anyhow::Error::new(EnhancedErrorWrapper {
            message: final_display,
        })
    }
}

/// Wrapper for tag parsing errors with automatic example generation
#[allow(unused_variables)]
pub fn tag_parsing_error(
    tag_name: &str,
    message: &str,
    file_path: &str,
    suggestion: Option<&str>,
) -> anyhow::Error {
    {
        let use_color = std::env::var("NO_COLOR").is_err() && atty::is(atty::Stream::Stderr);

        // Color codes
        let bold_red = if use_color { "\x1b[1;31m" } else { "" };
        let red = if use_color { "\x1b[31m" } else { "" };
        let cyan = if use_color { "\x1b[36m" } else { "" };
        let blue_grey = if use_color { "\x1b[38;5;245m" } else { "" }; // lighter grey for source context
        let light_blue = if use_color { "\x1b[38;5;75m" } else { "" };
        let grey = if use_color { "\x1b[90m" } else { "" };
        let reset = if use_color { "\x1b[0m" } else { "" };

        // Parse file path to extract line number if present
        let (actual_file_path, line_number) = if file_path.contains(':') {
            let parts: Vec<&str> = file_path.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(line_num) = parts[1].parse::<usize>() {
                    // TODO: PANIC POTENTIAL - parts[1] access without bounds check
                    (parts[0], Some(line_num))
                } else {
                    (file_path, None)
                }
            } else {
                (file_path, None)
            }
        } else {
            (file_path, None)
        };

        // Create error header
        let error_display = format!(
            "{}Tag error{}: {} @ {}{}{} {}(errno: ERR_4002){}\n",
            bold_red, reset, message, cyan, file_path, reset, grey, reset
        );

        // Add guidance
        let guidance = if let Some(suggest) = suggestion {
            format!("{}  -> {}{}\n", light_blue, suggest, reset)
        } else {
            format!("{}  -> invalid tag or syntax{}\n", light_blue, reset)
        };

        // Try to read source file and show context if we have a line number
        let context_display = if let Some(line_num) = line_number {
            if let Ok(content) = std::fs::read_to_string(actual_file_path) {
                let lines: Vec<&str> = content.lines().collect();
                let mut context = String::from("\n");

                // Show line before for context (if available)
                if line_num > 1 && line_num - 2 < lines.len() {
                    let prev_line = lines[line_num - 2]; // TODO: PANIC POTENTIAL - line_num could be < 2 causing underflow
                    context.push_str(&format!(
                        "{}{:4}{} | {}{}{}\n",
                        grey,
                        line_num - 1,
                        reset,
                        blue_grey,
                        prev_line,
                        reset
                    ));
                }

                // Show the problematic line - make line number red
                if line_num > 0 && line_num - 1 < lines.len() {
                    let error_line = lines[line_num - 1]; // TODO: PANIC POTENTIAL - line_num could be 0 causing underflow
                    context.push_str(&format!(
                        "{}{:4}{} | {}\n",
                        red, line_num, reset, error_line
                    ));

                    // Try to find the error column by looking for the problematic text
                    if let Some(col) = error_line.find("source:") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^{}\n", spaces, red, reset));
                    } else if let Some(col) = error_line.find("transform:") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^^^^{}\n", spaces, red, reset));
                    } else if let Some(col) = error_line.find("condition:") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^^^^{}\n", spaces, red, reset));
                    } else if let Some(col) = error_line.find("!$mapp") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^{}\n", spaces, red, reset));
                    } else if message.contains("not a valid iidy tag") {
                        // Generic unknown tag highlighting - find any !$ tag
                        if let Some(col) = error_line.find("!$") {
                            let tag_end = error_line[col..]
                                .find(' ')
                                .unwrap_or(error_line.len() - col); // TODO: PANIC POTENTIAL - if col > error_line.len()
                            let spaces = " ".repeat(col);
                            let carets = "^".repeat(tag_end.min(10));
                            context.push_str(&format!(
                                "     | {}{}{}{}\n",
                                spaces, red, carets, reset
                            ));
                        }
                    } else if message.contains("not found in") {
                        // Property access error highlighting - find the include reference
                        if let Some(col) = error_line.find("!$ ") {
                            let include_start = col + 3; // Skip "!$ "
                            let include_end = error_line[include_start..]
                                .find(' ')
                                .unwrap_or(error_line.len() - include_start);
                            let spaces = " ".repeat(include_start);
                            let carets = "^".repeat(include_end.min(15));
                            context.push_str(&format!(
                                "     | {}{}{}{}\n",
                                spaces, red, carets, reset
                            ));
                        }
                    }
                }

                // Show line after for context (if available)
                if line_num < lines.len() {
                    let next_line = lines[line_num];
                    context.push_str(&format!(
                        "{}{:4}{} | {}{}{}\n",
                        grey,
                        line_num + 1,
                        reset,
                        blue_grey,
                        next_line,
                        reset
                    ));
                }

                context
            } else {
                String::from("\n")
            }
        } else {
            String::from("\n")
        };

        // Generate appropriate example based on tag name
        let example_display = match tag_name {
            "!$map" => format!(
                "\n{}   example:\n   !$map\n     items: [1, 2, 3]\n     template: \"{{{{item}}}}\"{}\n",
                light_blue, reset
            ),
            "!$if" => format!(
                "\n{}   example:\n   !$if\n     test: !$eq [\"prod\", \"{{{{env}}}}\"]\n     then: \"production\"\n     else: \"development\"{}\n",
                light_blue, reset
            ),
            "!$let" => format!(
                "\n{}   example:\n   !$let\n     var1: value1\n     var2: value2\n     in: \"{{{{var1}}}}-{{{{var2}}}}\"{}\n",
                light_blue, reset
            ),
            "!$merge" => format!(
                "\n{}   example:\n   !$merge\n     - {{key1: value1}}\n     - {{key2: value2}}\n     - {{key3: value3}}{}\n",
                light_blue, reset
            ),
            "!$concat" => format!(
                "\n{}   example:\n   !$concat\n     - [item1, item2]\n     - [item3, item4]\n     - [item5]{}\n",
                light_blue, reset
            ),
            "!$" | "!$include" => format!(
                "\n{}   example:\n   !$ variable_name{}\n",
                light_blue, reset
            ),
            "!$eq" => format!(
                "\n{}   example:\n   !$eq [\"{{{{env}}}}\", \"production\"]{}\n",
                light_blue, reset
            ),
            "!$split" => format!(
                "\n{}   example:\n   !$split [\",\", \"a,b,c\"]{}\n",
                light_blue, reset
            ),
            "!$join" => format!(
                "\n{}   example:\n   !$join [\",\", [\"a\", \"b\", \"c\"]]{}\n",
                light_blue, reset
            ),
            "!$groupBy" => format!(
                "\n{}   example:\n   !$groupBy\n     items: [{{name: \"a\", type: \"x\"}}, {{name: \"b\", type: \"x\"}}]\n     key: type\n     var: group\n     template: \"{{{{group.key}}}}: {{{{#each group.items}}}}{{{{name}}}}{{{{/each}}}}\"{}\n",
                light_blue, reset
            ),
            "!$concatMap" => format!(
                "\n{}   example:\n   !$concatMap\n     items: [1, 2, 3]\n     template: [\"{{{{item}}}}-a\", \"{{{{item}}}}-b\"]{}\n",
                light_blue, reset
            ),
            "!$mapListToHash" => format!(
                "\n{}   example:\n   !$mapListToHash\n     items: [{{\"key\": \"a\", \"value\": 1}}, {{\"key\": \"b\", \"value\": 2}}]\n     keyPath: key\n     valuePath: value{}\n",
                light_blue, reset
            ),
            // Only show examples for known iidy tags that commonly have errors
            _ if tag_name.starts_with("!$") => format!(
                "\n{}   example:\n   {}\n     <check documentation for proper syntax>{}\n",
                light_blue, tag_name, reset
            ),
            _ => String::new(),
        };

        let final_display = format!(
            "{}{}{}{}{}   For more info, run: iidy explain ERR_4002{}\n",
            error_display, guidance, context_display, example_display, light_blue, reset
        );

        anyhow::Error::new(EnhancedErrorWrapper {
            message: final_display,
        })
    }
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
    // Parse line number from file_path if present (e.g., "file.yaml:6")
    let (actual_file_path, provided_line_number) = if file_path.contains(':') {
        let parts: Vec<&str> = file_path.split(':').collect();
        if parts.len() >= 2 {
            if let Ok(line_num) = parts[1].parse::<usize>() {
                // TODO: PANIC POTENTIAL - parts[1] access without bounds check
                (parts[0], Some(line_num))
            } else {
                (file_path, None)
            }
        } else {
            (file_path, None)
        }
    } else {
        (file_path, None)
    };

    // Try to read the source file for context and find the line
    let (source_lines, line_number, column_number) = if let Ok(content) =
        std::fs::read_to_string(actual_file_path)
    {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // If we already have a line number from the caller, use it
        if let Some(line_num) = provided_line_number {
            // Try to find the column by looking for the tag that caused the error
            let column_num = if line_num > 0 && line_num <= lines.len() {
                let line_content = &lines[line_num - 1]; // TODO: PANIC POTENTIAL - line_num could be 0 causing underflow

                // Look for common iidy tag patterns based on context description
                if context_description.contains("!$split delimiter field") {
                    // Point to the first argument (delimiter) in !$split [delimiter, string]
                    if let Some(bracket_pos) = line_content.find('[') {
                        // Skip whitespace after bracket to point to actual argument
                        let after_bracket = &line_content[bracket_pos + 1..]; // TODO: PANIC POTENTIAL - if bracket_pos + 1 > line_content.len()
                        let whitespace_count = after_bracket
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        bracket_pos + 1 + whitespace_count + 1 // TODO: PANIC POTENTIAL - potential usize overflow on large files
                    } else {
                        line_content.find("!$split").map(|col| col + 8).unwrap_or(0)
                    }
                } else if context_description.contains("!$split string field") {
                    // Point to the second argument (string) in !$split [delimiter, string]
                    if let Some(comma_pos) = line_content.find(',') {
                        // Find the start of the second argument after the comma
                        let after_comma = &line_content[comma_pos + 1..]; // TODO: PANIC POTENTIAL - if comma_pos + 1 > line_content.len()
                        let whitespace_count = after_comma
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        comma_pos + 1 + whitespace_count + 1 // TODO: PANIC POTENTIAL - potential usize overflow on large files
                    } else {
                        line_content.find("!$split").map(|col| col + 8).unwrap_or(0)
                    }
                } else if context_description.contains("!$join delimiter argument") {
                    // Point to the first argument (delimiter) in !$join [delimiter, array]
                    if let Some(bracket_pos) = line_content.find('[') {
                        bracket_pos + 1 // Point to start of first argument
                    } else {
                        line_content.find("!$join").map(|col| col + 7).unwrap_or(0)
                    }
                } else if context_description.contains("!$join sequence argument") {
                    // Point to the second argument (array) in !$join [delimiter, array]
                    if let Some(bracket_pos) = line_content.find('[') {
                        // Find the comma that separates arguments (not inside quoted strings)
                        let after_bracket = &line_content[bracket_pos + 1..]; // TODO: PANIC POTENTIAL - if bracket_pos + 1 > line_content.len()
                        let mut in_quotes = false;
                        let mut quote_char = '"';
                        let mut separator_pos = None;

                        for (i, ch) in after_bracket.char_indices() {
                            match ch {
                                '"' | '\'' if !in_quotes => {
                                    in_quotes = true;
                                    quote_char = ch;
                                }
                                c if in_quotes && c == quote_char => {
                                    in_quotes = false;
                                }
                                ',' if !in_quotes => {
                                    separator_pos = Some(bracket_pos + 1 + i);
                                    break;
                                }
                                _ => {}
                            }
                        }

                        if let Some(comma_pos) = separator_pos {
                            // Find the start of the second argument after the separator comma
                            let after_comma = &line_content[comma_pos + 1..]; // TODO: PANIC POTENTIAL - if comma_pos + 1 > line_content.len()
                            let whitespace_count = after_comma
                                .chars()
                                .take_while(|c| c.is_whitespace())
                                .count();
                            comma_pos + 1 + whitespace_count + 1 // TODO: PANIC POTENTIAL - potential usize overflow on large files
                        } else {
                            line_content.find("!$join").map(|col| col + 7).unwrap_or(0)
                        }
                    } else {
                        line_content.find("!$join").map(|col| col + 7).unwrap_or(0)
                    }
                } else if context_description.contains("!$groupBy items field") {
                    // Point to the items field value, not the tag
                    if let Some(items_pos) = line_content.find("items:") {
                        let after_items = &line_content[items_pos + 6..]; // TODO: PANIC POTENTIAL - if items_pos + 6 > line_content.len() // Skip "items:"
                        let whitespace_count = after_items
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        items_pos + 6 + whitespace_count + 1 // TODO: PANIC POTENTIAL - potential usize overflow on large files
                    } else {
                        line_content
                            .find("!$groupBy")
                            .map(|col| col + 9)
                            .unwrap_or(0)
                    }
                } else if context_description.contains("!$mapListToHash items field") {
                    // Point to the items field value, not the tag
                    if let Some(items_pos) = line_content.find("items:") {
                        let after_items = &line_content[items_pos + 6..]; // TODO: PANIC POTENTIAL - if items_pos + 6 > line_content.len() // Skip "items:"
                        let whitespace_count = after_items
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        items_pos + 6 + whitespace_count + 1 // TODO: PANIC POTENTIAL - potential usize overflow on large files
                    } else {
                        line_content
                            .find("!$mapListToHash")
                            .map(|col| col + 15)
                            .unwrap_or(0)
                    }
                } else if context_description.contains("!$fromPairs source field") {
                    // Point to the source field value, not the tag
                    if let Some(source_pos) = line_content.find("source:") {
                        let after_source = &line_content[source_pos + 7..]; // TODO: PANIC POTENTIAL - if source_pos + 7 > line_content.len() // Skip "source:"
                        let whitespace_count = after_source
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        source_pos + 7 + whitespace_count + 1 // TODO: PANIC POTENTIAL - potential usize overflow on large files
                    } else {
                        line_content
                            .find("!$fromPairs")
                            .map(|col| col + 12)
                            .unwrap_or(0)
                    }
                } else if context_description.contains("!$fromPairs source item") {
                    // Point to the problematic item in the source array
                    line_content
                        .find("!$fromPairs")
                        .map(|col| col + 12)
                        .unwrap_or(0)
                } else if context_description.contains("!$map items field") {
                    line_content.find("!$map").map(|col| col + 6).unwrap_or(0) // +6 to point after "!$map "
                } else if context_description.contains("!$merge") {
                    line_content.find("!$merge").map(|col| col + 8).unwrap_or(0) // +8 to point after "!$merge "
                } else {
                    // Generic fallback - look for any !$ tag
                    line_content.find("!$").map(|col| col + 2).unwrap_or(0)
                }
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

                    // Skip comment lines (lines that start with #)
                    if trimmed_line.starts_with('#') {
                        return None;
                    }

                    if context_description.contains("!$split") && line.contains("!$split") {
                        let line_num = idx + 1;
                        let column_num = if context_description.contains("delimiter field") {
                            line.find('[')
                                .map(|bracket_pos| {
                                    let after_bracket = &line[bracket_pos + 1..];
                                    let whitespace_count = after_bracket
                                        .chars()
                                        .take_while(|c| c.is_whitespace())
                                        .count();
                                    bracket_pos + 1 + whitespace_count + 1
                                })
                                .unwrap_or_else(|| {
                                    line.find("!$split").map(|col| col + 8).unwrap_or(0)
                                })
                        } else if context_description.contains("string field") {
                            line.find(',')
                                .map(|comma_pos| {
                                    let after_comma = &line[comma_pos + 1..];
                                    let whitespace_count = after_comma
                                        .chars()
                                        .take_while(|c| c.is_whitespace())
                                        .count();
                                    comma_pos + 1 + whitespace_count + 1
                                })
                                .unwrap_or_else(|| {
                                    line.find("!$split").map(|col| col + 8).unwrap_or(0)
                                })
                        } else {
                            line.find("!$split").map(|col| col + 8).unwrap_or(0)
                        };
                        Some((line_num, column_num))
                    } else if context_description.contains("!$join") && line.contains("!$join") {
                        let line_num = idx + 1;
                        let column_num = if context_description.contains("delimiter argument") {
                            line.find('[').map(|col| col + 1).unwrap_or_else(|| {
                                line.find("!$join").map(|col| col + 7).unwrap_or(0)
                            })
                        } else if context_description.contains("sequence argument") {
                            // Find the comma that separates arguments (not inside quoted strings)
                            if let Some(bracket_pos) = line.find('[') {
                                let after_bracket = &line[bracket_pos + 1..];
                                let mut in_quotes = false;
                                let mut quote_char = '"';
                                let mut separator_pos = None;

                                for (i, ch) in after_bracket.char_indices() {
                                    match ch {
                                        '"' | '\'' if !in_quotes => {
                                            in_quotes = true;
                                            quote_char = ch;
                                        }
                                        c if in_quotes && c == quote_char => {
                                            in_quotes = false;
                                        }
                                        ',' if !in_quotes => {
                                            separator_pos = Some(bracket_pos + 1 + i);
                                            break;
                                        }
                                        _ => {}
                                    }
                                }

                                if let Some(comma_pos) = separator_pos {
                                    let after_comma = &line[comma_pos + 1..];
                                    let whitespace_count = after_comma
                                        .chars()
                                        .take_while(|c| c.is_whitespace())
                                        .count();
                                    comma_pos + 1 + whitespace_count + 1
                                } else {
                                    line.find("!$join").map(|col| col + 7).unwrap_or(0)
                                }
                            } else {
                                line.find("!$join").map(|col| col + 7).unwrap_or(0)
                            }
                        } else {
                            line.find("!$join").map(|col| col + 7).unwrap_or(0)
                        };
                        Some((line_num, column_num))
                    } else if context_description.contains("!$groupBy")
                        && line.contains("!$groupBy")
                    {
                        if context_description.contains("items field") {
                            // Look for the items field on subsequent lines
                            for (next_idx, next_line) in lines.iter().enumerate().skip(idx + 1) {
                                if let Some(items_pos) = next_line.find("items:") {
                                    let after_items = &next_line[items_pos + 6..];
                                    let whitespace_count = after_items
                                        .chars()
                                        .take_while(|c| c.is_whitespace())
                                        .count();
                                    return Some((
                                        next_idx + 1,
                                        items_pos + 6 + whitespace_count + 1,
                                    ));
                                }
                                // Stop looking if we hit another tag or the end of the current structure
                                if next_line.trim_start().starts_with('!')
                                    || next_line.trim().is_empty()
                                {
                                    break;
                                }
                            }
                            // Fallback to tag location
                            Some((
                                idx + 1,
                                line.find("!$groupBy").map(|col| col + 9).unwrap_or(0),
                            ))
                        } else {
                            Some((
                                idx + 1,
                                line.find("!$groupBy").map(|col| col + 9).unwrap_or(0),
                            ))
                        }
                    } else if context_description.contains("!$mapListToHash")
                        && line.contains("!$mapListToHash")
                    {
                        if context_description.contains("items field") {
                            // Look for the items field on subsequent lines
                            for (next_idx, next_line) in lines.iter().enumerate().skip(idx + 1) {
                                if let Some(items_pos) = next_line.find("items:") {
                                    let after_items = &next_line[items_pos + 6..];
                                    let whitespace_count = after_items
                                        .chars()
                                        .take_while(|c| c.is_whitespace())
                                        .count();
                                    return Some((
                                        next_idx + 1,
                                        items_pos + 6 + whitespace_count + 1,
                                    ));
                                }
                                // Stop looking if we hit another tag or the end of the current structure
                                if next_line.trim_start().starts_with('!')
                                    || next_line.trim().is_empty()
                                {
                                    break;
                                }
                            }
                            // Fallback to tag location
                            Some((
                                idx + 1,
                                line.find("!$mapListToHash")
                                    .map(|col| col + 15)
                                    .unwrap_or(0),
                            ))
                        } else {
                            Some((
                                idx + 1,
                                line.find("!$mapListToHash")
                                    .map(|col| col + 15)
                                    .unwrap_or(0),
                            ))
                        }
                    } else if context_description.contains("!$fromPairs")
                        && line.contains("!$fromPairs")
                    {
                        if context_description.contains("source field") {
                            // Look for the source field on subsequent lines
                            for (next_idx, next_line) in lines.iter().enumerate().skip(idx + 1) {
                                if let Some(source_pos) = next_line.find("source:") {
                                    let after_source = &next_line[source_pos + 7..];
                                    let whitespace_count = after_source
                                        .chars()
                                        .take_while(|c| c.is_whitespace())
                                        .count();
                                    return Some((
                                        next_idx + 1,
                                        source_pos + 7 + whitespace_count + 1,
                                    ));
                                }
                                // Stop looking if we hit another tag or the end of the current structure
                                if next_line.trim_start().starts_with('!')
                                    || next_line.trim().is_empty()
                                {
                                    break;
                                }
                            }
                            // Fallback to tag location
                            Some((
                                idx + 1,
                                line.find("!$fromPairs").map(|col| col + 12).unwrap_or(0),
                            ))
                        } else {
                            Some((
                                idx + 1,
                                line.find("!$fromPairs").map(|col| col + 12).unwrap_or(0),
                            ))
                        }
                    } else if context_description.contains("!$map") && line.contains("!$map") {
                        Some((idx + 1, line.find("!$map").map(|col| col + 6).unwrap_or(0)))
                    } else if context_description.contains("!$merge") && line.contains("!$merge") {
                        Some((
                            idx + 1,
                            line.find("!$merge").map(|col| col + 8).unwrap_or(0),
                        ))
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

    let enhanced_display = error.display_with_context(source_lines.as_deref()); // TODO: PANIC POTENTIAL - display generation could panic during error creation
    anyhow::Error::new(EnhancedErrorWrapper {
        message: enhanced_display,
    })
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
    // Parse line number from file_path if present (e.g., "file.yaml:6")
    let (actual_file_path, provided_line_number) = if file_path.contains(':') {
        let parts: Vec<&str> = file_path.split(':').collect();
        if parts.len() >= 2 {
            if let Ok(line_num) = parts[1].parse::<usize>() {
                // TODO: PANIC POTENTIAL - parts[1] access without bounds check
                (parts[0], Some(line_num))
            } else {
                (file_path, None)
            }
        } else {
            (file_path, None)
        }
    } else {
        (file_path, None)
    };

    // Try to read the source file for context and find the line
    let (source_lines, line_number, column_number) =
        if let Ok(content) = std::fs::read_to_string(actual_file_path) {
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

            // Always search for the actual CloudFormation tag location, ignoring provided line number
            // because it's often inaccurate for CloudFormation tags
            let mut found_line = 0;
            let mut found_col = 1;

            for (idx, line) in lines.iter().enumerate() {
                // Look for CloudFormation tag, but exclude comments
                let trimmed_line = line.trim_start();
                if !trimmed_line.starts_with('#') {
                    if let Some(tag_col) = line.find(&format!("!{}", tag_name)) {
                        // Make sure this is a real CloudFormation tag (followed by space or [)
                        let tag_end = tag_col + tag_name.len() + 1; // +1 for "!"
                        if tag_end < line.len() {
                            let next_char = line.chars().nth(tag_end); // TODO: PANIC POTENTIAL - chars().nth() with byte vs char index mismatch
                            if matches!(next_char, Some(' ') | Some('[') | Some('\t')) {
                                found_line = idx + 1;

                                // Find the value after the tag
                                let after_tag = &line[tag_end..]; // TODO: PANIC POTENTIAL - if tag_end > line.len()
                                let value_start =
                                    after_tag.chars().take_while(|c| c.is_whitespace()).count();
                                found_col = tag_end + value_start + 1; // +1 for 1-based indexing
                                break;
                            }
                        }
                    }
                }
            }

            if found_line > 0 {
                (Some(lines), found_line, found_col)
            } else {
                // Fallback to provided line number if tag not found
                let fallback_line = provided_line_number.unwrap_or(1);
                (Some(lines), fallback_line, 1)
            }
        } else {
            (None, 1, 1)
        };

    let location = SourceLocation::new(actual_file_path, line_number, column_number, yaml_path);
    let error = EnhancedPreprocessingError::cloudformation_validation(tag_name, message, location);

    let enhanced_display = error.display_with_context(source_lines.as_deref()); // TODO: PANIC POTENTIAL - display generation could panic during error creation
    anyhow::Error::new(EnhancedErrorWrapper {
        message: enhanced_display,
    })
}

/// Error for query/JMESPath failures on a resolved variable lookup.
///
/// Formats in the same style as variable-not-found errors with source context.
pub fn lookup_query_error(
    variable_path: &str,
    message: &str,
    file_path: &str,
    line_number: usize,
    available_keys: &[String],
) -> anyhow::Error {
    let use_color = std::env::var("NO_COLOR").is_err() && atty::is(atty::Stream::Stderr);

    let bold_red = if use_color { "\x1b[1;31m" } else { "" };
    let red = if use_color { "\x1b[31m" } else { "" };
    let cyan = if use_color { "\x1b[36m" } else { "" };
    let blue_grey = if use_color { "\x1b[38;5;245m" } else { "" };
    let light_blue = if use_color { "\x1b[38;5;75m" } else { "" };
    let grey = if use_color { "\x1b[90m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    let location = if line_number > 0 {
        format!("{}:{}:0", file_path, line_number)
    } else {
        file_path.to_string()
    };

    let mut output = format!(
        "{}Lookup error{}: {} @ {}{}{} {}(errno: ERR_2001){}\n",
        bold_red, reset, message, cyan, location, reset, grey, reset,
    );
    output.push_str(&format!(
        "{}  -> query failed on variable '{}'{}\n",
        light_blue, variable_path, reset,
    ));

    // Source context
    if line_number > 0 {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            let lines: Vec<&str> = content.lines().collect();
            output.push('\n');

            if line_number > 1 && line_number - 2 < lines.len() {
                output.push_str(&format!(
                    "{}{:4}{} | {}{}{}\n",
                    grey, line_number - 1, reset, blue_grey, lines[line_number - 2], reset,
                ));
            }
            if line_number > 0 && line_number - 1 < lines.len() {
                output.push_str(&format!(
                    "{}{:4}{} | {}\n",
                    red, line_number, reset, lines[line_number - 1],
                ));
            }
            if line_number < lines.len() {
                output.push_str(&format!(
                    "{}{:4}{} | {}{}{}\n",
                    grey, line_number + 1, reset, blue_grey, lines[line_number], reset,
                ));
            }
            output.push('\n');
        }
    }

    if !available_keys.is_empty() {
        let mut sorted = available_keys.to_vec();
        sorted.sort();
        output.push_str(&format!(
            "{}   available keys: {}{}\n",
            light_blue,
            sorted.join(", "),
            reset,
        ));
    }

    output.push_str(&format!(
        "\n{}   For more info: iidy explain ERR_2001{}\n",
        light_blue, reset,
    ));

    anyhow::Error::new(EnhancedErrorWrapper { message: output })
}
