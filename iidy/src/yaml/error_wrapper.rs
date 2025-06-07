use crate::yaml::enhanced_errors::{EnhancedPreprocessingError, SourceLocation};

/// Marker trait for enhanced errors that should be displayed without prefix
pub trait EnhancedError {
    fn is_enhanced(&self) -> bool { true }
}

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

impl EnhancedError for EnhancedErrorWrapper {}

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
        let (source_lines, line_number, column_number) = if let Ok(content) = std::fs::read_to_string(actual_file_path) {
            let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
            
            // If we already have a line number from the caller, use it and find the column
            if let Some(line_num) = provided_line_number {
                let column_num = if line_num > 0 && line_num <= lines.len() {
                    let line_content = &lines[line_num - 1];
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
                let (line_num, column_num) = lines.iter().enumerate().find_map(|(idx, line)| {
                    if let Some(col) = line.find(&format!("!$ {}", variable)) {
                        Some((idx + 1, col + 4)) // +4 to point after "!$ "
                    } else if let Some(col) = line.find(&format!("!${}", variable)) {
                        Some((idx + 1, col + 3)) // +3 to point after "!$"
                    } else if let Some(col) = line.find(&format!("{{{{{}}}}}", variable)) {
                        Some((idx + 1, col + 2)) // +2 to point after "{{"
                    } else {
                        None
                    }
                }).unwrap_or((0, 0));
                
                (Some(lines), line_num, column_num)
            }
        } else {
            (None, 0, 0)
        };
        
        let location = SourceLocation::new(actual_file_path, line_number, column_number, yaml_path);
        let error = EnhancedPreprocessingError::variable_not_found(variable, location, available_vars);
        
        let enhanced_display = error.display_with_context(source_lines.as_deref());
        anyhow::Error::new(EnhancedErrorWrapper { message: enhanced_display })
    }
}

/// Wrapper for type mismatch errors
#[allow(unused_variables)]
pub fn type_mismatch_error(
    expected: &str,
    found: &str,
    file_path: &str,
    yaml_path: &str,
    context: &str,
) -> anyhow::Error {
    {
        // Enhanced error format
        let location = SourceLocation::new(file_path, 0, 0, yaml_path);
        let error = EnhancedPreprocessingError::type_mismatch(expected, found, location, context);
        let enhanced_display = format!("{}", error);
        anyhow::Error::new(EnhancedErrorWrapper { message: enhanced_display })
    }
}

/// Wrapper for missing required field errors
#[allow(unused_variables)]
pub fn missing_required_field_error(
    tag_name: &str,
    missing_field: &str,
    file_path: &str,
    yaml_path: &str,
    required_fields: Vec<String>,
) -> anyhow::Error {
    {
        // Enhanced error format - parse line number from file_path if present
        let (actual_file_path, line_number) = if file_path.contains(':') {
            let parts: Vec<&str> = file_path.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(line_num) = parts[1].parse::<usize>() {
                    (parts[0], line_num)
                } else {
                    (file_path, 0)
                }
            } else {
                (file_path, 0)
            }
        } else {
            (file_path, 0)
        };
        
        // Try to read source file and show context if we have a line number
        let source_lines = if line_number > 0 {
            std::fs::read_to_string(actual_file_path).ok()
                .map(|content| content.lines().map(|s| s.to_string()).collect::<Vec<_>>())
        } else {
            None
        };
        
        let location = SourceLocation::new(actual_file_path, line_number, 0, yaml_path);
        let error = EnhancedPreprocessingError::missing_required_field(
            tag_name,
            missing_field,
            location,
            required_fields,
        );
        let enhanced_display = error.display_with_context(source_lines.as_deref());
        anyhow::Error::new(EnhancedErrorWrapper { message: enhanced_display })
    }
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
        let error_display = format!("{}Syntax error{}: {} @ {}{}:{}:{}{} {}(errno: IY1001){}\n", 
            bold_red, reset, short_msg, cyan, file_path, line_num, col_num, reset, grey, reset);
        
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
                output.push_str(&format!("{}{:4}{} | {}{}{}\n", grey, line_num - 1, reset, blue_grey, prev_line, reset));
            }
            
            // Show the problematic line - make line number red to draw attention
            let line_content = &source_lines[line_num - 1];
            output.push_str(&format!("{}{:4}{} | {}\n", red, line_num, reset, line_content));
            
            // Show caret if we have column info
            if col_num > 0 && col_num <= line_content.len() {
                let spaces = " ".repeat(col_num - 1); // column offset
                output.push_str(&format!("     | {}{}^{}\n", spaces, red, reset));
            }
            
            // Show line after for context (if available) - in blue-grey with grey line number
            if line_num < source_lines.len() {
                let next_line = &source_lines[line_num];
                output.push_str(&format!("{}{:4}{} | {}{}{}\n", grey, line_num + 1, reset, blue_grey, next_line, reset));
            }
            
            // Add specific help for common issues
            if error_msg.contains("did not find expected key") && line_content.contains("!$") {
                output.push_str(&format!("\n{}   fix: put the inner tag in a list to separate it from the outer tag{}\n", light_blue, reset));
                output.push_str(&format!("{}   example: !$not [!$eq [\"a\", \"b\"]]{}\n", light_blue, reset));
            }
            
            output.push_str(&format!("\n{}   For more info, run: iidy explain IY1001{}\n", light_blue, reset));
            output
        } else {
            format!("{}{}\n{}   For more info, run: iidy explain IY1001{}\n", error_display, guidance_line, light_blue, reset)
        };
        
        anyhow::Error::new(EnhancedErrorWrapper { message: final_display })
    }
}

/// Wrapper for tag parsing errors
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
        let error_display = format!("{}Tag error{}: {} @ {}{}{} {}(errno: IY4002){}\n", 
            bold_red, reset, message, cyan, file_path, reset, grey, reset);
        
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
                    let prev_line = lines[line_num - 2];
                    context.push_str(&format!("{}{:4}{} | {}{}{}\n", grey, line_num - 1, reset, blue_grey, prev_line, reset));
                }
                
                // Show the problematic line - make line number red
                if line_num > 0 && line_num - 1 < lines.len() {
                    let error_line = lines[line_num - 1];
                    context.push_str(&format!("{}{:4}{} | {}\n", red, line_num, reset, error_line));
                    
                    // Try to find the error column by looking for the problematic text
                    if let Some(col) = error_line.find("source:") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^{}\n", spaces, red, reset));
                    } else if let Some(col) = error_line.find("transform:") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^^^^{}\n", spaces, red, reset));
                    } else if let Some(col) = error_line.find("!$mapp") {
                        let spaces = " ".repeat(col);
                        context.push_str(&format!("     | {}{}^^^^^^{}\n", spaces, red, reset));
                    } else if message.contains("not a valid iidy tag") {
                        // Generic unknown tag highlighting - find any !$ tag
                        if let Some(col) = error_line.find("!$") {
                            let tag_end = error_line[col..].find(' ').unwrap_or(error_line.len() - col);
                            let spaces = " ".repeat(col);
                            let carets = "^".repeat(tag_end.min(10));
                            context.push_str(&format!("     | {}{}{}{}\n", spaces, red, carets, reset));
                        }
                    } else if message.contains("not found in") {
                        // Property access error highlighting - find the include reference
                        if let Some(col) = error_line.find("!$ ") {
                            let include_start = col + 3; // Skip "!$ "
                            let include_end = error_line[include_start..].find(' ').unwrap_or(error_line.len() - include_start);
                            let spaces = " ".repeat(include_start);
                            let carets = "^".repeat(include_end.min(15));
                            context.push_str(&format!("     | {}{}{}{}\n", spaces, red, carets, reset));
                        }
                    }
                }
                
                // Show line after for context (if available)
                if line_num < lines.len() {
                    let next_line = lines[line_num];
                    context.push_str(&format!("{}{:4}{} | {}{}{}\n", grey, line_num + 1, reset, blue_grey, next_line, reset));
                }
                
                context
            } else {
                String::from("\n")
            }
        } else {
            String::from("\n")
        };
        
        let final_display = format!("{}{}{}{}   For more info, run: iidy explain IY4002{}\n", 
            error_display, guidance, context_display, light_blue, reset);
        
        anyhow::Error::new(EnhancedErrorWrapper { message: final_display })
    }
}
