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
        Self { line, column, offset }
    }
    
    pub fn start() -> Self {
        Self { line: 1, column: 1, offset: 0 }
    }
}

/// Trait for finding positions of YAML elements within source text
pub trait LocationFinder {
    /// Find the position of a tag within the current YAML path context
    /// This should be more accurate than find_position_of when there are multiple occurrences
    fn find_tag_position_in_context(&self, source: &str, yaml_path: &str, tag_name: &str) -> Option<Position>;
    
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
    fn find_tag_position_in_context(&self, source: &str, yaml_path: &str, tag_name: &str) -> Option<Position> {
        if yaml_path.is_empty() {
            // No context path, fall back to first occurrence
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
            if let Some(context_pos) = self.find_yaml_key_context(source, yaml_path, clean_segment, tag_name) {
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

impl TreeSitterLocationFinder {
    /// Create a new tree-sitter location finder
    pub fn new() -> Self {
        Self
    }
}

impl LocationFinder for TreeSitterLocationFinder {
    fn find_tag_position_in_context(&self, source: &str, yaml_path: &str, tag_name: &str) -> Option<Position> {
        use crate::yaml::tree_sitter_location::find_tag_position_with_tree_sitter;
        
        match find_tag_position_with_tree_sitter(source, yaml_path, tag_name) {
            Ok(pos) => Some(Position::new(pos.line, pos.column, pos.offset)),
            Err(_) => {
                // Fallback to manual approach if tree-sitter fails
                let manual_finder = ManualLocationFinder;
                manual_finder.find_tag_position_in_context(source, yaml_path, tag_name)
            }
        }
    }
    
    fn find_position_of(&self, source: &str, search_text: &str) -> Option<Position> {
        if let Some(found_offset) = source.find(search_text) {
            Some(self.offset_to_position(source, found_offset))
        } else {
            None
        }
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
    /// Find a tag within the context of a specific YAML key
    /// Looks for patterns like "key: !$tag" or "key:\n  field: !$tag"
    /// Takes into account array indices in the path for more precise matching
    fn find_yaml_key_context(&self, source: &str, yaml_path: &str, key_name: &str, tag_name: &str) -> Option<Position> {
        // Extract array index if present in the yaml_path
        let array_index = extract_array_index_from_path(yaml_path);
        
        // Find all occurrences of the key
        let mut key_offset = 0;
        let mut key_occurrence = 0;
        
        while let Some(key_pos) = self.find_position_of_from_offset(source, &format!("{}:", key_name), key_offset) {
            // If we have an array index, we want to find the nth occurrence of this key
            // where n matches the array index
            if let Some(target_index) = array_index {
                if key_occurrence != target_index {
                    key_occurrence += 1;
                    key_offset = key_pos.offset + key_name.len() + 1;
                    continue;
                }
            }
            
            // Look for the tag within a reasonable distance after this key
            let search_start = key_pos.offset;
            let search_end = std::cmp::min(source.len(), search_start + 1000); // max search window
            
            // Look for the tag after this key position
            if let Some(tag_pos) = self.find_position_of_from_offset(source, tag_name, search_start) {
                if tag_pos.offset < search_end {
                    // Found a tag after this key within reasonable distance
                    return Some(tag_pos);
                }
            }
            
            // Move to next key occurrence
            key_occurrence += 1;
            key_offset = key_pos.offset + key_name.len() + 1; // +1 for the ':'
        }
        
        None
    }
    
    /// Find a tag at approximately the right depth based on YAML path
    /// Uses indentation and nesting level as hints, handling inconsistent indentation
    fn find_tag_at_approximate_depth(&self, source: &str, tag_name: &str, expected_depth: usize) -> Option<Position> {
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
    fn find_position_of_from_offset(&self, source: &str, search_text: &str, start_offset: usize) -> Option<Position> {
        let search_start = start_offset.min(source.len());
        
        if let Some(found_offset) = source[search_start..].find(search_text) {
            let absolute_offset = search_start + found_offset;
            Some(self.offset_to_position(source, absolute_offset))
        } else {
            None
        }
    }
}

/// Extract array index from the YAML path if present
/// For example: "ListOperations[2].operation" -> Some(2)
fn extract_array_index_from_path(yaml_path: &str) -> Option<usize> {
    // Look for pattern like "[number]" in the path
    if let Some(start) = yaml_path.find('[') {
        if let Some(end) = yaml_path[start..].find(']') {
            let index_str = &yaml_path[start + 1..start + end];
            return index_str.parse().ok();
        }
    }
    None
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
fn select_best_depth_candidate(candidates: Vec<(Position, usize)>, expected_depth: usize) -> Option<Position> {
    if candidates.is_empty() {
        return None;
    }
    
    // Score each candidate by how close their depth is to expected
    let mut scored_candidates: Vec<_> = candidates.into_iter()
        .map(|(pos, depth)| {
            let depth_diff = (depth as i32 - expected_depth as i32).abs();
            let score = match depth_diff {
                0 => 100.0,                    // Perfect match
                1 => 80.0,                     // Very close
                2 => 60.0,                     // Close
                3 => 40.0,                     // Somewhat close
                _ => 20.0 / (depth_diff as f64), // Decreasing score for larger differences
            };
            (pos, score)
        })
        .collect();
    
    // Sort by score (higher is better)
    scored_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    scored_candidates.into_iter().next().map(|(pos, _)| pos)
}