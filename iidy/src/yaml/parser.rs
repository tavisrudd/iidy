//! YAML parser with custom tag support
//! 
//! Implements parsing of YAML documents with iidy's custom preprocessing tags

use anyhow::{anyhow, Result};
use serde_yaml::{Mapping, Sequence, Value};

use crate::yaml::ast::*;
use std::collections::HashSet;

/// Information about indentation on a line
#[derive(Debug, Clone, Copy)]
struct IndentInfo {
    spaces: usize,
    tabs: usize,
}

/// Configuration for ParseContext behavior
#[derive(Debug, Clone)]
pub struct ParseConfig {
    /// Maximum distance to search for tags after a key (default: 1000)
    pub max_search_window: usize,
    /// Expected indentation size for depth estimation (default: 2)
    pub indent_size: usize,
    /// Maximum occurrences to check before giving up (default: 50)
    pub max_occurrence_checks: usize,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            max_search_window: 1000,
            indent_size: 2,
            max_occurrence_checks: 50,
        }
    }
}

impl ParseConfig {
    /// Create a new config with custom indentation size
    pub fn with_indent_size(indent_size: usize) -> Self {
        Self {
            indent_size,
            ..Default::default()
        }
    }
    
    /// Auto-detect indentation size from source text
    pub fn auto_detect_indent(source: &str) -> Self {
        let detected_indent = detect_indent_size(source);
        Self::with_indent_size(detected_indent)
    }
}

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

/// Parsing context that tracks location and position for better error reporting
#[derive(Debug, Clone)]
pub struct ParseContext {
    /// Full file location (can be local path, S3 URL, HTTPS URL, etc.)
    pub file_location: String,
    /// Original source text
    pub source: String,
    /// Current position in the document
    pub position: Position,
    /// Current path within the YAML structure (e.g., "Resources.MyBucket.Properties")
    pub yaml_path: String,
    /// Configuration for parsing behavior
    config: ParseConfig,
}

impl ParseContext {
    /// Create a new parsing context with default configuration
    pub fn new(file_location: impl Into<String>, source: impl Into<String>) -> Self {
        Self::with_config(file_location, source, ParseConfig::default())
    }
    
    /// Create a new parsing context with custom configuration
    pub fn with_config(
        file_location: impl Into<String>, 
        source: impl Into<String>,
        config: ParseConfig
    ) -> Self {
        Self {
            file_location: file_location.into(),
            source: source.into(),
            position: Position::start(),
            yaml_path: String::new(),
            config,
        }
    }
    
    
    /// Get the formatted location string for error messages
    pub fn location_string(&self) -> String {
        format!("{}:{}:{}", self.file_location, self.position.line, self.position.column)
    }
    
    /// Create a new context with an extended YAML path
    pub fn with_path(&self, segment: &str) -> Self {
        let new_path = if self.yaml_path.is_empty() {
            segment.to_string()
        } else {
            format!("{}.{}", self.yaml_path, segment)
        };
        
        Self {
            file_location: self.file_location.clone(),
            source: self.source.clone(),
            position: self.position.clone(),
            yaml_path: new_path,
            config: self.config.clone(),
        }
    }
    
    /// Create a new context with an array index path segment
    pub fn with_array_index(&self, index: usize) -> Self {
        let new_path = if self.yaml_path.is_empty() {
            format!("[{}]", index)
        } else {
            format!("{}[{}]", self.yaml_path, index)
        };
        
        Self {
            file_location: self.file_location.clone(),
            source: self.source.clone(),
            position: self.position.clone(),
            yaml_path: new_path,
            config: self.config.clone(),
        }
    }
    
    /// Update position to a specific line and column
    pub fn with_position(&self, line: usize, column: usize, offset: usize) -> Self {
        Self {
            file_location: self.file_location.clone(),
            source: self.source.clone(),
            position: Position::new(line, column, offset),
            yaml_path: self.yaml_path.clone(),
            config: self.config.clone(),
        }
    }
    
    /// Convert offset to line and column (simple implementation for error handling)
    fn offset_to_position(&self, offset: usize) -> Position {
        let mut line = 1;
        let mut column = 1;
        
        for (i, ch) in self.source.char_indices() {
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
    
    /// Find the line and column for a specific substring in the source
    /// This is more reliable than the current find_tag_line_number approach
    pub fn find_position_of(&self, search_text: &str) -> Option<Position> {
        self.find_position_of_from_offset(search_text, 0)
    }
    
    /// Find the position of a tag within the current YAML path context
    /// This is more accurate than find_position_of when there are multiple occurrences
    /// It attempts to find the tag occurrence that matches the current YAML path context
    pub fn find_tag_position_in_context(&self, tag_name: &str) -> Option<Position> {
        if self.yaml_path.is_empty() {
            // No context path, fall back to first occurrence
            return self.find_position_of(tag_name);
        }
        
        // Strategy: Find the YAML structure around our current path, then find the tag within that context
        let path_segments: Vec<&str> = self.yaml_path.split('.').collect();
        
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
            if let Some(context_pos) = self.find_yaml_key_context(clean_segment, tag_name) {
                return Some(context_pos);
            }
        }
        
        // If we can't find a specific context, try to use the path depth to find the right occurrence
        self.find_tag_at_approximate_depth(tag_name, path_segments.len())
    }
    
    /// Find a tag within the context of a specific YAML key
    /// Looks for patterns like "key: !$tag" or "key:\n  field: !$tag"
    /// Takes into account array indices in the path for more precise matching
    fn find_yaml_key_context(&self, key_name: &str, tag_name: &str) -> Option<Position> {
        // Extract array index if present in the yaml_path
        let array_index = self.extract_array_index_from_path();
        
        // Find all occurrences of the key
        let mut key_offset = 0;
        let mut key_occurrence = 0;
        
        while let Some(key_pos) = self.find_position_of_from_offset(&format!("{}:", key_name), key_offset) {
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
            let search_end = std::cmp::min(
                self.source.len(),
                search_start + self.config.max_search_window
            );
            
            // Look for the tag after this key position
            if let Some(tag_pos) = self.find_position_of_from_offset(tag_name, search_start) {
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
    
    /// Extract array index from the YAML path if present
    /// For example: "ListOperations[2].operation" -> Some(2)
    pub fn extract_array_index_from_path(&self) -> Option<usize> {
        // Look for pattern like "[number]" in the path
        if let Some(start) = self.yaml_path.find('[') {
            if let Some(end) = self.yaml_path[start..].find(']') {
                let index_str = &self.yaml_path[start + 1..start + end];
                return index_str.parse().ok();
            }
        }
        None
    }
    
    /// Find a tag at approximately the right depth based on YAML path
    /// Uses indentation and nesting level as hints, handling inconsistent indentation
    fn find_tag_at_approximate_depth(&self, tag_name: &str, expected_depth: usize) -> Option<Position> {
        let mut offset = 0;
        let mut occurrence_count = 0;
        let mut candidates = Vec::new();
        
        // Collect all occurrences with their estimated depths
        while let Some(tag_pos) = self.find_position_of_from_offset(tag_name, offset) {
            if let Some(line_content) = self.get_line_content(tag_pos.line) {
                let indent_info = self.analyze_line_indentation(line_content);
                let estimated_depth = self.estimate_depth_from_indent(indent_info, expected_depth);
                
                candidates.push((tag_pos.clone(), estimated_depth));
            }
            
            occurrence_count += 1;
            offset = tag_pos.offset + tag_name.len();
            
            // Safety limit to avoid infinite loops
            if occurrence_count > self.config.max_occurrence_checks {
                break;
            }
        }
        
        // Find the best match based on depth and other heuristics
        self.select_best_depth_candidate(candidates, expected_depth)
            .or_else(|| self.find_position_of(tag_name))
    }
    
    /// Analyze indentation characteristics of a line
    fn analyze_line_indentation(&self, line: &str) -> IndentInfo {
        let mut spaces = 0;
        let mut tabs = 0;
        
        for ch in line.chars() {
            match ch {
                ' ' => spaces += 1,
                '\t' => tabs += 1,
                _ => break,
            }
        }
        
        IndentInfo { spaces, tabs }
    }
    
    /// Estimate depth from indentation, handling mixed tabs/spaces and inconsistent sizing
    fn estimate_depth_from_indent(&self, indent_info: IndentInfo, expected_depth: usize) -> usize {
        if indent_info.tabs > 0 && indent_info.spaces > 0 {
            // Mixed indentation - use heuristic
            // Treat each tab as equivalent to config.indent_size spaces
            let total_spaces = indent_info.spaces + (indent_info.tabs * self.config.indent_size);
            (total_spaces / self.config.indent_size).max(1)
        } else if indent_info.tabs > 0 {
            // Tab-based indentation
            indent_info.tabs.max(1)
        } else if indent_info.spaces > 0 {
            // Space-based indentation - try to auto-detect indent size if inconsistent
            let detected_indent = self.detect_likely_indent_size(indent_info.spaces, expected_depth);
            (indent_info.spaces / detected_indent).max(1)
        } else {
            // No indentation
            1
        }
    }
    
    /// Detect likely indent size based on current indentation and expected depth
    fn detect_likely_indent_size(&self, spaces: usize, expected_depth: usize) -> usize {
        if expected_depth <= 1 {
            return self.config.indent_size;
        }
        
        // Try to infer indent size: spaces / expected_depth
        let inferred = spaces / expected_depth;
        if inferred > 0 && inferred <= 8 {
            inferred
        } else {
            self.config.indent_size
        }
    }
    
    /// Select the best candidate based on depth matching and other factors
    fn select_best_depth_candidate(&self, candidates: Vec<(Position, usize)>, expected_depth: usize) -> Option<Position> {
        if candidates.is_empty() {
            return None;
        }
        
        // Score each candidate
        let mut scored_candidates: Vec<_> = candidates.into_iter()
            .map(|(pos, depth)| {
                let depth_score = self.calculate_depth_score(depth, expected_depth);
                let path_score = self.calculate_path_score(&pos);
                let total_score = depth_score + path_score;
                (pos, total_score)
            })
            .collect();
        
        // Sort by score (higher is better)
        scored_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        scored_candidates.into_iter().next().map(|(pos, _)| pos)
    }
    
    /// Calculate score for depth matching (higher = better match)
    fn calculate_depth_score(&self, actual_depth: usize, expected_depth: usize) -> f64 {
        let diff = (actual_depth as i32 - expected_depth as i32).abs();
        match diff {
            0 => 100.0,                    // Perfect match
            1 => 80.0,                     // Very close
            2 => 60.0,                     // Close
            3 => 40.0,                     // Somewhat close
            _ => 20.0 / (diff as f64),     // Decreasing score for larger differences
        }
    }
    
    /// Calculate score based on path context (higher = better match)
    fn calculate_path_score(&self, _pos: &Position) -> f64 {
        // Future enhancement: could analyze surrounding context
        // For now, just return neutral score
        0.0
    }
    
    /// Find position of text starting from a specific offset (for handling multiple matches)
    pub fn find_position_of_from_offset(&self, search_text: &str, start_offset: usize) -> Option<Position> {
        let search_start = start_offset.min(self.source.len());
        
        if let Some(found_offset) = self.source[search_start..].find(search_text) {
            let absolute_offset = search_start + found_offset;
            Some(self.offset_to_position(absolute_offset))
        } else {
            None
        }
    }
    
    /// Get the current line content for context in error messages
    pub fn current_line_content(&self) -> Option<&str> {
        self.get_line_content(self.position.line)
    }
    
    /// Get content of a specific line
    pub fn get_line_content(&self, line_number: usize) -> Option<&str> {
        self.source.lines().nth(line_number.saturating_sub(1))
    }
}

/// Auto-detect the most likely indentation size from source text
fn detect_indent_size(source: &str) -> usize {
    let mut indent_counts = std::collections::HashMap::new();
    let mut prev_indent = 0;
    
    for line in source.lines() {
        if line.trim().is_empty() {
            continue; // Skip empty lines
        }
        
        let current_indent = line.len() - line.trim_start().len();
        
        if current_indent > prev_indent {
            let diff = current_indent - prev_indent;
            *indent_counts.entry(diff).or_insert(0) += 1;
        }
        
        prev_indent = current_indent;
    }
    
    // Find the most common indentation increase
    indent_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(size, _)| size)
        .filter(|&size| size > 0 && size <= 8)
        .unwrap_or(2) // Default to 2 if no clear pattern
}

/// Validate that a mapping has exactly the required keys and optionally allowed keys, with no extras
/// Also provides helpful suggestions for common wrong field names
fn validate_exact_keys(
    map: &serde_yaml::Mapping,
    required_keys: &[&str],
    optional_keys: &[&str],
    tag_name: &str,
    context: &ParseContext,
) -> Result<()> {
    let provided_keys: HashSet<String> = map.keys()
        .filter_map(|k| if let Value::String(s) = k { Some(s.clone()) } else { None })
        .collect();
    
    let required_set: HashSet<String> = required_keys.iter().map(|s| s.to_string()).collect();
    let optional_set: HashSet<String> = optional_keys.iter().map(|s| s.to_string()).collect();
    let all_valid_keys: HashSet<String> = required_set.union(&optional_set).cloned().collect();
    
    // First, check for common wrong field names and provide specific suggestions
    let common_mistakes = [
        ("source", "items"),
        ("transform", "template"),
        ("condition", "test"),
    ];
    
    for (wrong_key, correct_key) in &common_mistakes {
        if provided_keys.contains(*wrong_key) && required_set.contains(*correct_key) && !provided_keys.contains(*correct_key) {
            use crate::yaml::error_wrapper::tag_parsing_error;
            
            // Use ParseContext to find the position of the wrong key in its current context
            let location = if let Some(position) = context.find_tag_position_in_context(&format!("{}:", wrong_key)) {
                format!("{}:{}:{}", context.file_location, position.line, position.column)
            } else {
                context.location_string()
            };
            
            let suggestion = format!("use '{}' instead of '{}' in {} tags", correct_key, wrong_key, tag_name);
            
            return Err(tag_parsing_error(tag_name, &format!("'{}' should be '{}'", wrong_key, correct_key), &location, Some(&suggestion)));
        }
    }
    
    // Check for missing required keys
    let missing: Vec<String> = required_set.difference(&provided_keys).cloned().collect();
    if !missing.is_empty() {
        use crate::yaml::error_wrapper::missing_required_field_error;
        
        // Use ParseContext to find the position of the tag in its current context
        let location = if let Some(position) = context.find_tag_position_in_context(tag_name) {
            format!("{}:{}:{}", context.file_location, position.line, position.column)
        } else {
            context.location_string()
        };
        
        return Err(missing_required_field_error(
            tag_name,
            &missing[0], // Show the first missing field
            &location,
            &context.yaml_path,
            required_keys.iter().map(|s| s.to_string()).collect()
        ));
    }
    
    // Check for extra keys
    let extra: Vec<String> = provided_keys.difference(&all_valid_keys).cloned().collect();
    if !extra.is_empty() {
        use crate::yaml::error_wrapper::tag_parsing_error;
        
        // Use ParseContext to find the position of the tag in its current context
        let location = if let Some(position) = context.find_tag_position_in_context(tag_name) {
            format!("{}:{}:{}", context.file_location, position.line, position.column)
        } else {
            context.location_string()
        };
        
        let all_keys: Vec<String> = required_keys.iter()
            .map(|s| s.to_string())
            .chain(optional_keys.iter().map(|s| format!("{} (optional)", s)))
            .collect();
        
        let suggestion = if extra.len() == 1 {
            format!("unexpected field '{}'. Valid fields are: {}", extra[0], all_keys.join(", "))
        } else {
            format!("unexpected fields: {}. Valid fields are: {}", extra.join(", "), all_keys.join(", "))
        };
        
        return Err(tag_parsing_error(tag_name, &suggestion, &location, None));
    }
    
    Ok(())
}

// Helper functions removed - ParseContext now handles position tracking


/// Parse YAML text with file context for better error reporting  
pub fn parse_yaml_with_custom_tags_from_file(input: &str, file_path: &str) -> Result<YamlAst> {
    let context = ParseContext::new(file_path, input);
    let value: Value = serde_yaml::from_str(input)
        .map_err(|e| crate::yaml::error_wrapper::yaml_syntax_error(e, file_path, input))?;
    convert_value_to_ast(value, &context)
}

/// Convert a serde_yaml::Value to our custom AST
fn convert_value_to_ast(value: Value, context: &ParseContext) -> Result<YamlAst> {
    match value {
        Value::Null => Ok(YamlAst::Null),
        Value::Bool(b) => Ok(YamlAst::Bool(b)),
        Value::Number(n) => Ok(YamlAst::Number(n)),
        Value::String(s) => Ok(YamlAst::String(s)),
        Value::Sequence(seq) => convert_sequence_to_ast(seq, context),
        Value::Mapping(map) => convert_mapping_to_ast(map, context),
        Value::Tagged(tagged) => parse_tagged_value(*tagged, context),
    }
}

/// Convert a YAML sequence to AST
fn convert_sequence_to_ast(seq: Sequence, context: &ParseContext) -> Result<YamlAst> {
    let mut ast_seq = Vec::new();
    for (index, item) in seq.into_iter().enumerate() {
        let item_context = context.with_array_index(index);
        ast_seq.push(convert_value_to_ast(item, &item_context)?);
    }
    Ok(YamlAst::Sequence(ast_seq))
}

/// Convert a YAML mapping to AST
fn convert_mapping_to_ast(map: Mapping, context: &ParseContext) -> Result<YamlAst> {
    // Check for special preprocessing keys like $imports, $defs
    if let Some(preprocessing_tag) = check_for_preprocessing_keys(&map)? {
        return Ok(YamlAst::PreprocessingTag(preprocessing_tag));
    }

    // Regular mapping
    let mut ast_map = Vec::new();
    for (key, value) in map {
        let key_ast = convert_value_to_ast(key, context)?;
        let value_context = if let YamlAst::String(key_str) = &key_ast {
            context.with_path(key_str)
        } else {
            context.clone()
        };
        let value_ast = convert_value_to_ast(value, &value_context)?;
        ast_map.push((key_ast, value_ast));
    }
    Ok(YamlAst::Mapping(ast_map))
}

/// Parse a tagged YAML value (handles !$ tags)
fn parse_tagged_value(tagged: serde_yaml::value::TaggedValue, context: &ParseContext) -> Result<YamlAst> {
    let tag = tagged.tag.to_string();
    let value = tagged.value;

    match tag.as_str() {
        "!$" | "!$include" => parse_include_tag(value, context),
        "!$if" => parse_if_tag(value, context),
        "!$map" => parse_map_tag(value, context),
        "!$merge" => parse_merge_tag(value, context),
        "!$concat" => parse_concat_tag(value, context),
        "!$let" => parse_let_tag(value, context),
        "!$eq" => parse_eq_tag(value, context),
        "!$not" => parse_not_tag(value, context),
        "!$split" => parse_split_tag(value, context),
        "!$join" => parse_join_tag(value, context),
        "!$concatMap" => parse_concat_map_tag(value, context),
        "!$mergeMap" => parse_merge_map_tag(value, context),
        "!$mapListToHash" => parse_map_list_to_hash_tag(value, context),
        "!$mapValues" => parse_map_values_tag(value, context),
        "!$groupBy" => parse_group_by_tag(value, context),
        "!$fromPairs" => parse_from_pairs_tag(value, context),
        "!$toYamlString" => parse_to_yaml_string_tag(value, context),
        "!$parseYaml" => parse_parse_yaml_tag(value, context),
        "!$toJsonString" => parse_to_json_string_tag(value, context),
        "!$parseJson" => parse_parse_json_tag(value, context),
        "!$escape" => parse_escape_tag(value, context),
        _ => {
            // Check for unknown iidy preprocessing tags (likely typos) with context
            if tag.starts_with("!$") {
                {
                    use crate::yaml::error_wrapper::tag_parsing_error;
                    
                    // Use ParseContext to find the position of the tag in its current context
                    let location = if let Some(position) = context.find_tag_position_in_context(&tag) {
                        format!("{}:{}:{}", context.file_location, position.line, position.column)
                    } else {
                        context.location_string()
                    };
                    
                    return Err(tag_parsing_error("unknown tag", &format!("'{}' is not a valid iidy tag", tag), &location, Some("check tag spelling or see documentation for valid tags")));
                }
            }
            
            // For other tags, reconstruct the tagged value and use the original parser
            let reconstructed = serde_yaml::value::TaggedValue {
                tag: tagged.tag,
                value,
            };
            parse_non_iidy_tagged_value(reconstructed, context)
        }
    }
}

fn parse_non_iidy_tagged_value(tagged: serde_yaml::value::TaggedValue, context: &ParseContext) -> Result<YamlAst> {
    let tag = tagged.tag.to_string();
    let value = tagged.value;
    // Unknown tag (like CloudFormation !Ref, !Sub), preserve with content processing
    // Strip the '!' prefix to get the actual tag name
    let tag_name = if tag.starts_with('!') {
        tag.strip_prefix('!').unwrap_or(&tag)
    } else {
        &tag
    };
    
    // Handle array syntax: !Ref [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let value = convert_value_to_ast(actual_value, context)?;
    Ok(YamlAst::UnknownYamlTag(UnknownTag { tag: tag_name.to_string(), value: Box::new(value) }))
}

/// Check if a mapping contains special preprocessing keys
fn check_for_preprocessing_keys(_map: &Mapping) -> Result<Option<PreprocessingTag>> {
    // For now, we'll focus on tagged values
    // Note: $imports, $defs are handled in the main preprocessing module, not in the parser
    Ok(None)
}

/// Parse !$ or !$include tag
fn parse_include_tag(value: Value, _context: &ParseContext) -> Result<YamlAst> {
    match value {
        Value::String(path) => Ok(YamlAst::PreprocessingTag(PreprocessingTag::Include(
            IncludeTag {
                path,
                query: None,
            },
        ))),
        Value::Mapping(map) => {
            let path = extract_string_field(&map, "path")?;
            let query = extract_optional_string_field(&map, "query");
            Ok(YamlAst::PreprocessingTag(PreprocessingTag::Include(
                IncludeTag { path, query },
            )))
        }
        _ => Err(anyhow!("Invalid include tag format")),
    }
}

/// Parse !$if tag
fn parse_if_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["test", "then"], &["else"], "!$if", context)?;
        
        let condition_val = map.get(&Value::String("test".to_string())).unwrap(); // Safe due to validation
        let then_val = map.get(&Value::String("then".to_string())).unwrap(); // Safe due to validation
        let else_val = map.get(&Value::String("else".to_string()));

        let condition = Box::new(convert_value_to_ast(condition_val.clone(), &context.with_path("test"))?);
        let then_value = Box::new(convert_value_to_ast(then_val.clone(), &context.with_path("then"))?);
        let else_value = if let Some(else_val) = else_val {
            Some(Box::new(convert_value_to_ast(else_val.clone(), &context.with_path("else"))?))  
        } else {
            None
        };

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::If(IfTag {
            test: condition,
            then_value,
            else_value,
        })))
    } else {
        Err(anyhow!("!$if requires a mapping with keys 'test', 'then', and optionally 'else'"))
    }
}

/// Parse !$map tag
fn parse_map_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var", "filter"], "!$map", context)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");
        
        // Optional filter
        let filter = if let Some(filter_val) = map.get(&Value::String("filter".to_string())) {
            Some(Box::new(convert_value_to_ast(filter_val.clone(), &context.with_path("filter"))?))  
        } else {
            None
        };

        let items = Box::new(convert_value_to_ast(items_val.clone(), &context.with_path("items"))?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), &context.with_path("template"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Map(MapTag {
            items,
            template,
            var: var_name,
            filter,
        })))
    } else {
        Err(anyhow!("Map tag must be a mapping"))
    }
}

/// Parse !$merge tag
fn parse_merge_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    match value {
        Value::Sequence(seq) => {
            let mut sources = Vec::new();
            for (index, item) in seq.into_iter().enumerate() {
                let item_context = context.with_array_index(index);
                sources.push(convert_value_to_ast(item, &item_context)?);
            }
            Ok(YamlAst::PreprocessingTag(PreprocessingTag::Merge(
                MergeTag { sources },
            )))
        }
        _ => Err(anyhow!("Merge tag must be a sequence")),
    }
}

/// Parse !$concat tag
fn parse_concat_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    match value {
        Value::Sequence(seq) => {
            let mut sources = Vec::new();
            for (index, item) in seq.into_iter().enumerate() {
                let item_context = context.with_array_index(index);
                sources.push(convert_value_to_ast(item, &item_context)?);
            }
            Ok(YamlAst::PreprocessingTag(PreprocessingTag::Concat(
                ConcatTag { sources },
            )))
        }
        _ => Err(anyhow!("Concat tag must be a sequence")),
    }
}

/// Parse !$let tag
fn parse_let_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys
        validate_exact_keys(&map, &["bindings", "expression"], &[], "!$let", context)?;
        
        let bindings_val = map.get(&Value::String("bindings".to_string())).unwrap(); // Safe due to validation
        let expression_val = map.get(&Value::String("expression".to_string())).unwrap(); // Safe due to validation

        let mut bindings = Vec::new();
        if let Value::Mapping(bindings_map) = bindings_val {
            let bindings_context = context.with_path("bindings");
            for (key, value) in bindings_map {
                if let Value::String(var_name) = key {
                    let var_context = bindings_context.with_path(var_name);
                    let var_value = convert_value_to_ast(value.clone(), &var_context)?;
                    bindings.push((var_name.clone(), var_value));
                }
            }
        }

        let expression = Box::new(convert_value_to_ast(expression_val.clone(), &context.with_path("expression"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Let(LetTag {
            bindings,
            expression,
        })))
    } else {
        Err(anyhow!("Let tag must be a mapping"))
    }
}

/// Parse !$eq tag
fn parse_eq_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Sequence(seq) = value {
        if seq.len() != 2 {
            return Err(anyhow!("Eq tag must have exactly 2 elements"));
        }
        let left = Box::new(convert_value_to_ast(seq[0].clone(), &context.with_array_index(0))?);
        let right = Box::new(convert_value_to_ast(seq[1].clone(), &context.with_array_index(1))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Eq(EqTag {
            left,
            right,
        })))
    } else {
        Err(anyhow!("Eq tag must be a sequence of 2 elements"))
    }
}

/// Parse !$not tag
fn parse_not_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$not [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let expression = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::Not(NotTag {
        expression,
    })))
}

/// Parse !$split tag
fn parse_split_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        let string_val = map.get(&Value::String("string".to_string()))
            .ok_or_else(|| anyhow!("Missing 'string' in split tag"))?;
        let delimiter = extract_string_field(&map, "delimiter")?;

        let string = Box::new(convert_value_to_ast(string_val.clone(), &context.with_path("string"))?);
        let delimiter = Box::new(YamlAst::String(delimiter));

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Split(
            SplitTag { delimiter, string },
        )))
    } else {
        Err(anyhow!("Split tag must be a mapping"))
    }
}

/// Parse !$join tag (expects [delimiter, array] format like iidy-js)
fn parse_join_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Sequence(seq) = value {
        if seq.len() != 2 {
            return Err(anyhow!("Join tag must be a sequence with two elements: [delimiter, array]"));
        }

        let delimiter = Box::new(convert_value_to_ast(seq[0].clone(), &context.with_array_index(0))?);
        let array = Box::new(convert_value_to_ast(seq[1].clone(), &context.with_array_index(1))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Join(JoinTag {
            delimiter,
            array,
        })))
    } else {
        Err(anyhow!("Join tag must be a sequence with format [delimiter, array]"))
    }
}

/// Helper to extract a required string field from a mapping
fn extract_string_field(map: &Mapping, field: &str) -> Result<String> {
    map.get(&Value::String(field.to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Missing or invalid '{}' field", field))
}

/// Helper to extract an optional string field from a mapping
fn extract_optional_string_field(map: &Mapping, field: &str) -> Option<String> {
    map.get(&Value::String(field.to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Parse !$concatMap tag
fn parse_concat_map_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var", "filter"], "!$concatMap", context)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");
        
        // Optional filter
        let filter = if let Some(filter_val) = map.get(&Value::String("filter".to_string())) {
            Some(Box::new(convert_value_to_ast(filter_val.clone(), &context.with_path("filter"))?))  
        } else {
            None
        };

        let items = Box::new(convert_value_to_ast(items_val.clone(), &context.with_path("items"))?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), &context.with_path("template"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::ConcatMap(ConcatMapTag {
            items,
            template,
            var: var_name,
            filter,
        })))
    } else {
        Err(anyhow!("ConcatMap tag must be a mapping"))
    }
}

/// Parse !$mergeMap tag
fn parse_merge_map_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var"], "!$mergeMap", context)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");

        let items = Box::new(convert_value_to_ast(items_val.clone(), &context.with_path("items"))?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), &context.with_path("template"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::MergeMap(MergeMapTag {
            items,
            template,
            var: var_name,
        })))
    } else {
        Err(anyhow!("MergeMap tag must be a mapping"))
    }
}

/// Parse !$mapListToHash tag
fn parse_map_list_to_hash_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var", "filter"], "!$mapListToHash", context)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");
        
        // Optional filter
        let filter = if let Some(filter_val) = map.get(&Value::String("filter".to_string())) {
            Some(Box::new(convert_value_to_ast(filter_val.clone(), &context.with_path("filter"))?))  
        } else {
            None
        };

        let items = Box::new(convert_value_to_ast(items_val.clone(), &context.with_path("items"))?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), &context.with_path("template"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::MapListToHash(MapListToHashTag {
            items,
            template,
            var: var_name,
            filter,
        })))
    } else {
        Err(anyhow!("MapListToHash tag must be a mapping"))
    }
}

/// Parse !$mapValues tag
fn parse_map_values_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var"], "!$mapValues", context)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");

        let items = Box::new(convert_value_to_ast(items_val.clone(), &context.with_path("items"))?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), &context.with_path("template"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::MapValues(MapValuesTag {
            items,
            template,
            var: var_name,
        })))
    } else {
        Err(anyhow!("MapValues tag must be a mapping"))
    }
}

/// Parse !$groupBy tag
fn parse_group_by_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "key"], &["var", "template"], "!$groupBy", context)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let key_val = map.get(&Value::String("key".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");
        
        // Optional template
        let template = if let Some(template_val) = map.get(&Value::String("template".to_string())) {
            Some(Box::new(convert_value_to_ast(template_val.clone(), &context.with_path("template"))?))  
        } else {
            None
        };

        let items = Box::new(convert_value_to_ast(items_val.clone(), &context.with_path("items"))?);
        let key = Box::new(convert_value_to_ast(key_val.clone(), &context.with_path("key"))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::GroupBy(GroupByTag {
            items,
            key,
            var: var_name,
            template,
        })))
    } else {
        Err(anyhow!("GroupBy tag must be a mapping"))
    }
}

/// Parse !$fromPairs tag
fn parse_from_pairs_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$fromPairs [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let source = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::FromPairs(FromPairsTag {
        source,
    })))
}

/// Parse !$toYamlString tag
fn parse_to_yaml_string_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$toYamlString [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let data = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ToYamlString(ToYamlStringTag {
        data,
    })))
}

/// Parse !$parseYaml tag
fn parse_parse_yaml_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$parseYaml [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let yaml_string = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ParseYaml(ParseYamlTag {
        yaml_string,
    })))
}

/// Parse !$toJsonString tag
fn parse_to_json_string_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$toJsonString [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let data = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ToJsonString(ToJsonStringTag {
        data,
    })))
}

/// Parse !$parseJson tag
fn parse_parse_json_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$parseJson [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let json_string = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ParseJson(ParseJsonTag {
        json_string,
    })))
}

/// Parse !$escape tag
fn parse_escape_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    // Handle array syntax: !$escape [expression] should extract the expression
    let (actual_value, value_context) = match value {
        Value::Sequence(seq) if seq.len() == 1 => {
            (seq.into_iter().next().unwrap(), context.with_array_index(0))
        },
        other => (other, context.clone()),
    };
    
    let content = Box::new(convert_value_to_ast(actual_value, &value_context)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::Escape(EscapeTag {
        content,
    })))
}
