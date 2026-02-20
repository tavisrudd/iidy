/// Shared display helpers for error formatting.
///
/// Internal helpers used by enhanced.rs and wrapper.rs to avoid
/// duplicating source context rendering, color setup, and file path parsing.
/// Parse a file path that may contain a line number suffix (e.g., "file.yaml:42").
///
/// Returns `(file_path, Some(line_num))` or `(original_input, None)`.
pub(crate) fn parse_file_location(file_path: &str) -> (&str, Option<usize>) {
    if let Some(colon_pos) = file_path.find(':') {
        let rest = &file_path[colon_pos + 1..];
        let num_str = rest.split(':').next().unwrap_or(rest);
        if let Ok(line_num) = num_str.parse::<usize>() {
            return (&file_path[..colon_pos], Some(line_num));
        }
    }
    (file_path, None)
}

/// Parse a file path, read the file, and return source lines.
///
/// Combines `parse_file_location` with `fs::read_to_string` -- the common preamble
/// in most wrapper functions.
pub(crate) fn read_source_lines(file_path: &str) -> (&str, Option<usize>, Option<Vec<String>>) {
    let (actual_file, line_num) = parse_file_location(file_path);
    let lines = std::fs::read_to_string(actual_file)
        .ok()
        .map(|content| content.lines().map(|s| s.to_string()).collect());
    (actual_file, line_num, lines)
}

/// Parse a file path with optional line and column: "file.yaml:42:10" -> ("file.yaml", Some(42), Some(10)).
pub(crate) fn parse_file_location_full(file_path: &str) -> (&str, Option<usize>, Option<usize>) {
    if let Some(colon_pos) = file_path.find(':') {
        let rest = &file_path[colon_pos + 1..];
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if let Ok(line_num) = parts[0].parse::<usize>() {
            let col = parts.get(1).and_then(|s| s.parse::<usize>().ok());
            return (&file_path[..colon_pos], Some(line_num), col);
        }
    }
    (file_path, None, None)
}

/// Terminal color codes for error display, respecting NO_COLOR and tty detection.
pub(crate) struct ErrorColors {
    pub(crate) bold_red: &'static str,
    pub(crate) red: &'static str,
    pub(crate) cyan: &'static str,
    pub(crate) blue_grey: &'static str,
    pub(crate) light_blue: &'static str,
    pub(crate) grey: &'static str,
    pub(crate) reset: &'static str,
}

impl ErrorColors {
    pub(crate) fn detect() -> Self {
        let use_color = std::env::var("NO_COLOR").is_err() && atty::is(atty::Stream::Stderr);
        if use_color {
            Self {
                bold_red: "\x1b[1;31m",
                red: "\x1b[31m",
                cyan: "\x1b[36m",
                blue_grey: "\x1b[38;5;245m",
                light_blue: "\x1b[38;5;75m",
                grey: "\x1b[90m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                bold_red: "",
                red: "",
                cyan: "",
                blue_grey: "",
                light_blue: "",
                grey: "",
                reset: "",
            }
        }
    }
}

/// Render source context: previous line, current line (highlighted), optional caret, next line.
///
/// - `line_num`: 1-based line number of the error. Returns empty if 0 or out of bounds.
/// - `column`: 1-based column for caret placement. 0 means no caret.
/// - `span_len`: number of caret characters. 0 treated as 1.
/// - `inline_desc`: text shown after carets (empty = none).
pub(crate) fn format_source_context<S: AsRef<str>>(
    lines: &[S],
    line_num: usize,
    column: usize,
    span_len: usize,
    inline_desc: &str,
    colors: &ErrorColors,
) -> String {
    if line_num == 0 || line_num > lines.len() {
        return String::new();
    }

    let mut output = String::new();

    // Previous line
    if line_num > 1 {
        let prev_line = lines[line_num - 2].as_ref();
        output.push_str(&format!(
            "{}{:4}{} | {}{}{}\n",
            colors.grey,
            line_num - 1,
            colors.reset,
            colors.blue_grey,
            prev_line,
            colors.reset
        ));
    }

    // Current line (red line number for attention)
    let line_content = lines[line_num - 1].as_ref();
    output.push_str(&format!(
        "{}{:4}{} | {}\n",
        colors.red, line_num, colors.reset, line_content
    ));

    // Caret
    if column > 0 && column <= line_content.len() {
        let spaces = " ".repeat(column - 1);
        let effective_span = if span_len == 0 {
            1
        } else {
            span_len.min(line_content.len() - column + 1)
        };
        let carets = "^".repeat(effective_span.max(1));
        output.push_str(&format!(
            "     | {}{}{}{}",
            spaces, colors.red, carets, colors.reset
        ));
        if !inline_desc.is_empty() {
            output.push_str(&format!(
                " {}{}{}",
                colors.blue_grey, inline_desc, colors.reset
            ));
        }
        output.push('\n');
    }

    // Next line
    if line_num < lines.len() {
        let next_line = lines[line_num].as_ref();
        output.push_str(&format!(
            "{}{:4}{} | {}{}{}\n",
            colors.grey,
            line_num + 1,
            colors.reset,
            colors.blue_grey,
            next_line,
            colors.reset
        ));
    }

    output
}

/// Find the column position (1-based) for a tag error caret based on context_description.
///
/// Used by type_mismatch_error_impl to locate the specific part of a line that caused
/// the type error. Returns 0 when no position can be determined.
pub(crate) fn find_tag_column(line_content: &str, context_description: &str) -> usize {
    if context_description.contains("!$split delimiter field") {
        find_after_bracket(line_content).unwrap_or_else(|| tag_fallback(line_content, "!$split", 8))
    } else if context_description.contains("!$split string field") {
        find_after_simple_comma(line_content)
            .unwrap_or_else(|| tag_fallback(line_content, "!$split", 8))
    } else if context_description.contains("!$join delimiter argument") {
        line_content
            .find('[')
            .map(|p| p + 1)
            .unwrap_or_else(|| tag_fallback(line_content, "!$join", 7))
    } else if context_description.contains("!$join sequence argument") {
        find_second_bracket_argument(line_content)
            .unwrap_or_else(|| tag_fallback(line_content, "!$join", 7))
    } else if context_description.contains("!$groupBy items field") {
        find_after_keyword(line_content, "items:", 6)
            .unwrap_or_else(|| tag_fallback(line_content, "!$groupBy", 9))
    } else if context_description.contains("!$mapListToHash items field") {
        find_after_keyword(line_content, "items:", 6)
            .unwrap_or_else(|| tag_fallback(line_content, "!$mapListToHash", 15))
    } else if context_description.contains("!$fromPairs source field") {
        find_after_keyword(line_content, "source:", 7)
            .unwrap_or_else(|| tag_fallback(line_content, "!$fromPairs", 12))
    } else if context_description.contains("!$fromPairs source item") {
        tag_fallback(line_content, "!$fromPairs", 12)
    } else if context_description.contains("!$map items field") {
        tag_fallback(line_content, "!$map", 6)
    // Tag-family fallbacks: catch context descriptions that mention the tag
    // but don't match a specific variant above (e.g., "!$join sequence item")
    } else if context_description.contains("!$split") {
        tag_fallback(line_content, "!$split", 8)
    } else if context_description.contains("!$join") {
        tag_fallback(line_content, "!$join", 7)
    } else if context_description.contains("!$groupBy") {
        tag_fallback(line_content, "!$groupBy", 9)
    } else if context_description.contains("!$mapListToHash") {
        tag_fallback(line_content, "!$mapListToHash", 15)
    } else if context_description.contains("!$fromPairs") {
        tag_fallback(line_content, "!$fromPairs", 12)
    } else if context_description.contains("!$merge") {
        tag_fallback(line_content, "!$merge", 8)
    } else if context_description.contains("!$map") {
        tag_fallback(line_content, "!$map", 6)
    } else {
        line_content.find("!$").map(|col| col + 2).unwrap_or(0)
    }
}

/// Search subsequent lines after a tag for a field keyword (e.g., "items:" after "!$groupBy").
///
/// Returns `(1-based line_num, 1-based column)` if found, or None.
pub(crate) fn search_field_on_subsequent_lines<S: AsRef<str>>(
    lines: &[S],
    tag_line_idx: usize,
    field_keyword: &str,
    context_description: &str,
) -> Option<(usize, usize)> {
    for (next_idx, next_line) in lines.iter().enumerate().skip(tag_line_idx + 1) {
        let next_str = next_line.as_ref();
        if next_str.contains(field_keyword) {
            return Some((next_idx + 1, find_tag_column(next_str, context_description)));
        }
        if next_str.trim_start().starts_with('!') || next_str.trim().is_empty() {
            break;
        }
    }
    None
}

/// Find the 1-based column of a variable reference in a line.
///
/// Searches for `{{variable}}`, `!$ variable`, and `!$variable` patterns.
/// Returns 0 when not found.
pub(crate) fn find_variable_column(line: &str, variable: &str) -> usize {
    if let Some(col) = line.find(&format!("!$ {variable}")) {
        col + 4
    } else if let Some(col) = line.find(&format!("!${variable}")) {
        col + 3
    } else if let Some(col) = line.find(&format!("{{{{{variable}}}}}")) {
        col + 2
    } else {
        0
    }
}

/// Tag patterns for searching source lines when no line number is provided.
/// Each entry: (tag_name, optional (context_keyword, field_keyword) for sub-field search).
const TAG_SEARCH_PATTERNS: &[(&str, Option<(&str, &str)>)] = &[
    ("!$split", None),
    ("!$join", None),
    ("!$groupBy", Some(("items field", "items:"))),
    ("!$mapListToHash", Some(("items field", "items:"))),
    ("!$fromPairs", Some(("source field", "source:"))),
    ("!$map", None),
    ("!$merge", None),
];

/// Search source lines for a tag mentioned in `context_description`, handling
/// optional sub-field searches on subsequent lines.
///
/// Returns `(1-based line, 1-based column)` or `(0, 0)` if not found.
pub(crate) fn search_for_tag_line<S: AsRef<str>>(
    lines: &[S],
    context_description: &str,
) -> (usize, usize) {
    lines
        .iter()
        .enumerate()
        .find_map(|(idx, line)| {
            let line_str = line.as_ref();
            if line_str.trim_start().starts_with('#') {
                return None;
            }
            for &(tag, field_search) in TAG_SEARCH_PATTERNS {
                if context_description.contains(tag) && line_str.contains(tag) {
                    if let Some((field_ctx, field_kw)) = field_search {
                        if context_description.contains(field_ctx) {
                            if let Some(result) = search_field_on_subsequent_lines(
                                lines,
                                idx,
                                field_kw,
                                context_description,
                            ) {
                                return Some(result);
                            }
                        }
                    }
                    return Some((idx + 1, find_tag_column(line_str, context_description)));
                }
            }
            None
        })
        .unwrap_or((0, 0))
}

// -- private helpers --

/// Find 1-based column of first non-whitespace char after `[`.
fn find_after_bracket(line: &str) -> Option<usize> {
    let bracket_pos = line.find('[')?;
    let after = line.get(bracket_pos + 1..)?;
    let ws = after.chars().take_while(|c| c.is_whitespace()).count();
    Some(bracket_pos + 1 + ws + 1)
}

/// Find 1-based column of first non-whitespace char after the first `,`.
fn find_after_simple_comma(line: &str) -> Option<usize> {
    let comma_pos = line.find(',')?;
    let after = line.get(comma_pos + 1..)?;
    let ws = after.chars().take_while(|c| c.is_whitespace()).count();
    Some(comma_pos + 1 + ws + 1)
}

/// Find 1-based column of first non-whitespace char after a keyword (e.g., "items:").
fn find_after_keyword(line: &str, keyword: &str, keyword_len: usize) -> Option<usize> {
    let pos = line.find(keyword)?;
    let after = line.get(pos + keyword_len..)?;
    let ws = after.chars().take_while(|c| c.is_whitespace()).count();
    Some(pos + keyword_len + ws + 1)
}

/// Find 1-based column of the second argument in a bracket expression like `[first, second]`,
/// handling quoted strings so commas inside quotes are ignored.
fn find_second_bracket_argument(line: &str) -> Option<usize> {
    let bracket_pos = line.find('[')?;
    let after_bracket = line.get(bracket_pos + 1..)?;
    let mut in_quotes = false;
    let mut quote_char = '"';

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
                let comma_pos = bracket_pos + 1 + i;
                let after_comma = line.get(comma_pos + 1..)?;
                let ws = after_comma
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .count();
                return Some(comma_pos + 1 + ws + 1);
            }
            _ => {}
        }
    }
    None
}

/// Fallback: find a tag name in the line and return 1-based column after it.
fn tag_fallback(line: &str, tag: &str, offset: usize) -> usize {
    line.find(tag).map(|col| col + offset).unwrap_or(0)
}

/// Compute caret (1-based column, span_len) for tag parsing errors.
/// Returns (0, 0) when no caret position can be determined.
pub(crate) fn tag_error_caret(error_line: &str, message: &str) -> (usize, usize) {
    if let Some(col) = error_line.find("source:") {
        (col + 1, 6)
    } else if let Some(col) = error_line.find("transform:") {
        (col + 1, 9)
    } else if let Some(col) = error_line.find("condition:") {
        (col + 1, 9)
    } else if let Some(col) = error_line.find("!$mapp") {
        (col + 1, 6)
    } else if message.contains("not a valid iidy tag") {
        if let Some(col) = error_line.find("!$") {
            let tag_end = error_line
                .get(col..)
                .map(|s| s.find(' ').unwrap_or(s.len()))
                .unwrap_or(0);
            (col + 1, tag_end.min(10))
        } else {
            (0, 0)
        }
    } else if message.contains("not found in") {
        if let Some(col) = error_line.find("!$ ") {
            let include_start = col + 3;
            let include_end = error_line
                .get(include_start..)
                .map(|s| s.find(' ').unwrap_or(s.len()))
                .unwrap_or(0);
            (include_start + 1, include_end.min(15))
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    }
}

/// Generate tag-specific usage examples.
pub(crate) fn tag_example(tag_name: &str, c: &ErrorColors) -> String {
    match tag_name {
        "!$map" => format!(
            "\n{}   example:\n   !$map\n     items: [1, 2, 3]\n     template: \"{{{{item}}}}\"{}\n",
            c.light_blue, c.reset
        ),
        "!$if" => format!(
            "\n{}   example:\n   !$if\n     test: !$eq [\"prod\", \"{{{{env}}}}\"]\n     then: \"production\"\n     else: \"development\"{}\n",
            c.light_blue, c.reset
        ),
        "!$let" => format!(
            "\n{}   example:\n   !$let\n     var1: value1\n     var2: value2\n     in: \"{{{{var1}}}}-{{{{var2}}}}\"{}\n",
            c.light_blue, c.reset
        ),
        "!$merge" => format!(
            "\n{}   example:\n   !$merge\n     - {{key1: value1}}\n     - {{key2: value2}}\n     - {{key3: value3}}{}\n",
            c.light_blue, c.reset
        ),
        "!$concat" => format!(
            "\n{}   example:\n   !$concat\n     - [item1, item2]\n     - [item3, item4]\n     - [item5]{}\n",
            c.light_blue, c.reset
        ),
        "!$" | "!$include" => format!(
            "\n{}   example:\n   !$ variable_name{}\n",
            c.light_blue, c.reset
        ),
        "!$eq" => format!(
            "\n{}   example:\n   !$eq [\"{{{{env}}}}\", \"production\"]{}\n",
            c.light_blue, c.reset
        ),
        "!$split" => format!(
            "\n{}   example:\n   !$split [\",\", \"a,b,c\"]{}\n",
            c.light_blue, c.reset
        ),
        "!$join" => format!(
            "\n{}   example:\n   !$join [\",\", [\"a\", \"b\", \"c\"]]{}\n",
            c.light_blue, c.reset
        ),
        "!$groupBy" => format!(
            "\n{}   example:\n   !$groupBy\n     items: [{{name: \"a\", type: \"x\"}}, {{name: \"b\", type: \"x\"}}]\n     key: type\n     var: group\n     template: \"{{{{group.key}}}}: {{{{#each group.items}}}}{{{{name}}}}{{{{/each}}}}\"{}\n",
            c.light_blue, c.reset
        ),
        "!$concatMap" => format!(
            "\n{}   example:\n   !$concatMap\n     items: [1, 2, 3]\n     template: [\"{{{{item}}}}-a\", \"{{{{item}}}}-b\"]{}\n",
            c.light_blue, c.reset
        ),
        "!$mapListToHash" => format!(
            "\n{}   example:\n   !$mapListToHash\n     items: [{{\"key\": \"a\", \"value\": 1}}, {{\"key\": \"b\", \"value\": 2}}]\n     keyPath: key\n     valuePath: value{}\n",
            c.light_blue, c.reset
        ),
        _ if tag_name.starts_with("!$") => format!(
            "\n{}   example:\n   {}\n     <check documentation for proper syntax>{}\n",
            c.light_blue, tag_name, c.reset
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_location_with_line() {
        assert_eq!(parse_file_location("file.yaml:42"), ("file.yaml", Some(42)));
    }

    #[test]
    fn test_parse_file_location_without_line() {
        assert_eq!(parse_file_location("file.yaml"), ("file.yaml", None));
    }

    #[test]
    fn test_parse_file_location_non_numeric() {
        assert_eq!(
            parse_file_location("file.yaml:abc"),
            ("file.yaml:abc", None)
        );
    }

    #[test]
    fn test_find_tag_column_split_delimiter() {
        let line = "  value: !$split [\"|\", \"a|b\"]";
        let col = find_tag_column(line, "!$split delimiter field");
        assert!(col > 0);
        // Should point after `[` and whitespace
        let bracket = line.find('[').unwrap();
        assert_eq!(col, bracket + 1 + 1); // +1 past bracket, +1 for 1-based
    }

    #[test]
    fn test_find_tag_column_generic_fallback() {
        let col = find_tag_column("  value: !$unknown stuff", "some unknown context");
        assert_eq!(col, "  value: !$".len()); // points after "!$", 0-based 10, 1-based... let me check
        // find("!$") returns 9 (0-based), + 2 = 11. But that's 1-based column 11.
        // Actually "  value: " is 9 chars, "!$" starts at 9, so find returns 9, +2 = 11.
        assert_eq!(col, 11);
    }

    #[test]
    fn test_find_second_bracket_argument() {
        let line = "!$join [\",\", [\"a\", \"b\"]]";
        let col = find_second_bracket_argument(line).unwrap();
        // The comma inside quotes should be skipped, finding the real separator
        assert!(col > 0);
    }
}
