//! Location finding strategies for YAML error reporting
//!
//! This module provides different strategies for finding precise positions of YAML tags
//! and elements within source text for better error reporting.

/// Position within a YAML document for precise error reporting
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Byte offset in the source text
    pub offset: usize,
}

impl Position {
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    pub fn start() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }
}

/// Trait for finding positions of YAML elements within source text
pub trait LocationFinder {
    /// Find the position of a tag within the current YAML path context
    /// This should be more accurate than find_position_of when there are multiple occurrences
    fn find_tag_position_in_context(
        &self,
        source: &str,
        yaml_path: &str,
        tag_name: &str,
    ) -> Option<Position>;

    /// Find the position of any text within the source
    fn find_position_of(&self, source: &str, search_text: &str) -> Option<Position>;

    /// Convert byte offset to line and column position
    fn offset_to_position(&self, source: &str, offset: usize) -> Position;
}

/// Manual position finding strategy (original implementation)
pub struct ManualLocationFinder;

/// Tree-sitter based position finding strategy  
pub struct TreeSitterLocationFinder;

impl LocationFinder for ManualLocationFinder {
    fn find_tag_position_in_context(
        &self,
        source: &str,
        yaml_path: &str,
        tag_name: &str,
    ) -> Option<Position> {
        use crate::debug::debug_log;

        debug_log!(
            "ManualLocationFinder: Finding tag '{}' in yaml_path '{}'",
            tag_name,
            yaml_path
        );

        if yaml_path.is_empty() {
            // No context path, fall back to first occurrence
            debug_log!(
                "ManualLocationFinder: yaml_path is empty, falling back to first occurrence search"
            );
            return self.find_position_of(source, tag_name);
        }

        // Strategy: Find the YAML structure around our current path, then find the tag within that context
        let path_segments: Vec<&str> = yaml_path.split('.').collect();

        // For simple cases, try to find a unique context pattern
        if let Some(last_segment) = path_segments.last() {
            // Clean up array indices like "MyKey[0]" -> "MyKey"
            let clean_segment = if let Some(bracket_pos) = last_segment.find('[') {
                &last_segment[..bracket_pos]
            } else {
                last_segment
            };

            // Look for patterns like "LastSegment: !$tag" or "LastSegment:\n  ...!$tag"
            // This helps distinguish between different occurrences
            if let Some(context_pos) =
                self.find_yaml_key_context(source, yaml_path, clean_segment, tag_name)
            {
                return Some(context_pos);
            }
        }

        // If we can't find a specific context, try to use the path depth to find the right occurrence
        self.find_tag_at_approximate_depth(source, tag_name, path_segments.len())
    }

    fn find_position_of(&self, source: &str, search_text: &str) -> Option<Position> {
        self.find_position_of_from_offset(source, search_text, 0)
    }

    fn offset_to_position(&self, source: &str, offset: usize) -> Position {
        let mut line = 1;
        let mut column = 1;

        for (i, ch) in source.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }

        Position::new(line, column, offset)
    }
}

impl Default for TreeSitterLocationFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeSitterLocationFinder {
    /// Create a new tree-sitter location finder
    pub fn new() -> Self {
        Self
    }
}

impl LocationFinder for TreeSitterLocationFinder {
    fn find_tag_position_in_context(
        &self,
        source: &str,
        yaml_path: &str,
        tag_name: &str,
    ) -> Option<Position> {
        use crate::debug::debug_log;
        use crate::yaml::tree_sitter_location::find_tag_position_with_tree_sitter;

        debug_log!(
            "TreeSitterLocationFinder: Finding tag '{}' in yaml_path '{}'",
            tag_name,
            yaml_path
        );

        match find_tag_position_with_tree_sitter(source, yaml_path, tag_name) {
            Ok(pos) => {
                debug_log!(
                    "TreeSitterLocationFinder: Found tag '{}' at line {}, column {}",
                    tag_name,
                    pos.line,
                    pos.column
                );
                Some(Position::new(pos.line, pos.column, pos.offset))
            }
            Err(_e) => {
                debug_log!(
                    "TreeSitterLocationFinder: Failed to find tag '{}' in path '{}': {}",
                    tag_name,
                    yaml_path,
                    _e
                );
                debug_log!("TreeSitterLocationFinder: Falling back to ManualLocationFinder");
                // Fallback to manual approach if tree-sitter fails
                let manual_finder = ManualLocationFinder;
                manual_finder.find_tag_position_in_context(source, yaml_path, tag_name)
            }
        }
    }

    fn find_position_of(&self, source: &str, search_text: &str) -> Option<Position> {
        source
            .find(search_text)
            .map(|found_offset| self.offset_to_position(source, found_offset))
    }

    fn offset_to_position(&self, source: &str, offset: usize) -> Position {
        let mut line = 1;
        let mut column = 1;

        for (i, ch) in source.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }

        Position::new(line, column, offset)
    }
}

// Manual implementation methods (extracted from ParseContext)
impl ManualLocationFinder {
    /// Find a tag within the context of a specific YAML key using path-aware search
    /// For paths like "Resources.Resource2.Properties.Value[0]", this will:
    /// 1. Find Resource2 within Resources section
    /// 2. Find Properties within that Resource2
    /// 3. Find Value within Properties
    /// 4. Find the nth array element if array index is present
    /// 5. Look for the tag within that context
    fn find_yaml_key_context(
        &self,
        source: &str,
        yaml_path: &str,
        _key_name: &str,
        tag_name: &str,
    ) -> Option<Position> {
        // Parse the full path to understand the context
        let path_segments: Vec<&str> = yaml_path.split('.').collect();
        if path_segments.is_empty() {
            return None;
        }

        // Extract array index from the last segment if present
        let (last_key, array_index) = if let Some(last_segment) = path_segments.last() {
            if let Some(bracket_start) = last_segment.find('[') {
                if let Some(bracket_end) = last_segment.find(']') {
                    let key_part = &last_segment[..bracket_start];
                    let index_str = &last_segment[bracket_start + 1..bracket_end];
                    let array_index = index_str.parse::<usize>().ok();
                    (key_part, array_index)
                } else {
                    (*last_segment, None)
                }
            } else {
                (*last_segment, None)
            }
        } else {
            return None;
        };

        // If this is a path with multiple segments, try to find the context by looking for
        // the preceding segments in sequence
        if path_segments.len() > 1 {
            let preceding_segments = &path_segments[..path_segments.len() - 1];

            // Look for patterns that match the path structure
            // For "Resources.Resource2.Properties.Value[0]", look for:
            // Resources: ... Resource2: ... Properties: ... Value: ... [array element 0] ... !$tag
            if let Some(context_pos) = self.find_nested_context(
                source,
                preceding_segments,
                last_key,
                array_index,
                tag_name,
            ) {
                return Some(context_pos);
            }
        }

        // Fallback: simple key search with array index
        self.find_simple_key_context(source, last_key, array_index, tag_name)
    }

    /// Find tag in nested YAML context by following path segments
    fn find_nested_context(
        &self,
        source: &str,
        preceding_segments: &[&str],
        target_key: &str,
        array_index: Option<usize>,
        tag_name: &str,
    ) -> Option<Position> {
        // Start from the beginning and try to find each segment in sequence
        let mut search_offset = 0;

        // Find each preceding segment in order
        for &segment in preceding_segments {
            let pattern = format!("{segment}:");
            if let Some(segment_pos) =
                self.find_position_of_from_offset(source, &pattern, search_offset)
            {
                search_offset = segment_pos.offset + pattern.len();
            } else {
                return None;
            }
        }

        // Now look for the target key after the last segment
        let target_pattern = format!("{target_key}:");
        let mut target_offset = search_offset;
        let mut target_occurrence = 0;

        // If we have an array index, we need to find the nth occurrence of the target key
        let target_occurrence_needed = array_index.unwrap_or(0);

        while let Some(target_pos) =
            self.find_position_of_from_offset(source, &target_pattern, target_offset)
        {
            if target_occurrence == target_occurrence_needed {
                // Look for the tag within a reasonable distance after this target key
                let search_start = target_pos.offset;
                let search_end = std::cmp::min(source.len(), search_start + 500); // reasonable search window

                if let Some(tag_pos) =
                    self.find_position_of_from_offset(source, tag_name, search_start)
                {
                    if tag_pos.offset < search_end {
                        return Some(tag_pos);
                    }
                }
                break;
            }

            target_occurrence += 1;
            target_offset = target_pos.offset + target_pattern.len();
        }

        None
    }

    /// Simple key context search (fallback method)
    fn find_simple_key_context(
        &self,
        source: &str,
        key_name: &str,
        array_index: Option<usize>,
        tag_name: &str,
    ) -> Option<Position> {
        // Find all occurrences of the key
        let mut key_offset = 0;
        let mut key_occurrence = 0;
        let target_occurrence = array_index.unwrap_or(0);

        while let Some(key_pos) =
            self.find_position_of_from_offset(source, &format!("{key_name}:"), key_offset)
        {
            if key_occurrence == target_occurrence {
                // Look for the tag within a reasonable distance after this key
                let search_start = key_pos.offset;
                let search_end = std::cmp::min(source.len(), search_start + 1000); // max search window

                // Look for the tag after this key position
                if let Some(tag_pos) =
                    self.find_position_of_from_offset(source, tag_name, search_start)
                {
                    if tag_pos.offset < search_end {
                        return Some(tag_pos);
                    }
                }
                break;
            }

            key_occurrence += 1;
            key_offset = key_pos.offset + key_name.len() + 1; // +1 for the ':'
        }

        None
    }

    /// Find a tag at approximately the right depth based on YAML path
    /// Uses indentation and nesting level as hints, handling inconsistent indentation
    fn find_tag_at_approximate_depth(
        &self,
        source: &str,
        tag_name: &str,
        expected_depth: usize,
    ) -> Option<Position> {
        let mut offset = 0;
        let mut occurrence_count = 0;
        let mut candidates = Vec::new();

        // Collect all occurrences with their estimated depths
        while let Some(tag_pos) = self.find_position_of_from_offset(source, tag_name, offset) {
            if let Some(line_content) = get_line_content(source, tag_pos.line) {
                let estimated_depth = estimate_depth_from_line(line_content, expected_depth);
                candidates.push((tag_pos.clone(), estimated_depth));
            }

            occurrence_count += 1;
            offset = tag_pos.offset + tag_name.len();

            // Safety limit to avoid infinite loops
            if occurrence_count > 50 {
                break;
            }
        }

        // Find the best match based on depth and other heuristics
        select_best_depth_candidate(candidates, expected_depth)
            .or_else(|| self.find_position_of(source, tag_name))
    }

    /// Find position of text starting from a specific offset (for handling multiple matches)
    fn find_position_of_from_offset(
        &self,
        source: &str,
        search_text: &str,
        start_offset: usize,
    ) -> Option<Position> {
        let search_start = start_offset.min(source.len());

        if let Some(found_offset) = source[search_start..].find(search_text) {
            let absolute_offset = search_start + found_offset;
            Some(self.offset_to_position(source, absolute_offset))
        } else {
            None
        }
    }
}

/// Get content of a specific line
fn get_line_content(source: &str, line_number: usize) -> Option<&str> {
    source.lines().nth(line_number.saturating_sub(1))
}

/// Estimate depth from line indentation
fn estimate_depth_from_line(line: &str, expected_depth: usize) -> usize {
    let indent_chars = line.len() - line.trim_start().len();

    if indent_chars == 0 {
        1
    } else {
        // Simple heuristic: assume 2-space indentation
        let estimated = (indent_chars / 2).max(1);
        // If we have expected depth, try to be smarter
        if expected_depth > 1 && indent_chars > 0 {
            let inferred_indent = indent_chars / expected_depth;
            if inferred_indent > 0 && inferred_indent <= 8 {
                expected_depth
            } else {
                estimated
            }
        } else {
            estimated
        }
    }
}

/// Select the best candidate based on depth matching
fn select_best_depth_candidate(
    candidates: Vec<(Position, usize)>,
    expected_depth: usize,
) -> Option<Position> {
    if candidates.is_empty() {
        return None;
    }

    // Score each candidate by how close their depth is to expected
    let mut scored_candidates: Vec<_> = candidates
        .into_iter()
        .map(|(pos, depth)| {
            let depth_diff = (depth as i32 - expected_depth as i32).abs();
            let score = match depth_diff {
                0 => 100.0,                      // Perfect match
                1 => 80.0,                       // Very close
                2 => 60.0,                       // Close
                3 => 40.0,                       // Somewhat close
                _ => 20.0 / (depth_diff as f64), // Decreasing score for larger differences
            };
            (pos, score)
        })
        .collect();

    // Sort by score (higher is better)
    scored_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    scored_candidates.into_iter().next().map(|(pos, _)| pos)
}
