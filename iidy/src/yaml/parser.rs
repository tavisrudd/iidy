//! YAML parser with custom tag support
//! 
//! Implements parsing of YAML documents with iidy's custom preprocessing tags

use anyhow::{anyhow, Result};
use serde_yaml::{Mapping, Sequence, Value};
use std::rc::Rc;

use crate::yaml::ast::*;
use crate::yaml::location::{LocationFinder, Position, TreeSitterLocationFinder, ManualLocationFinder};
use std::collections::HashSet;

/// Parsing context that tracks location and position for better error reporting
#[derive(Debug, Clone)]
pub struct ParseContext {
    /// Full file location (can be local path, S3 URL, HTTPS URL, etc.)
    pub file_location: Rc<str>,
    /// Original source text
    pub source: Rc<str>,
    /// Current path within the YAML structure (e.g., "Resources.MyBucket.Properties")
    pub yaml_path: String,
}

impl ParseContext {
    /// Create a new parsing context
    pub fn new(file_location: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            file_location: file_location.into().into(),
            source: source.into().into(),
            yaml_path: String::new(),
        }
    }
    
    
    /// Get the formatted location string for error messages
    pub fn location_string(&self) -> String {
        self.file_location.to_string()
    }
    
    /// Create a new context with an extended YAML path
    pub fn with_path(&self, segment: &str) -> Self {
        let new_path = if self.yaml_path.is_empty() {
            segment.to_string()
        } else {
            format!("{}.{}", self.yaml_path, segment)
        };
        
        Self {
            file_location: Rc::clone(&self.file_location),
            source: Rc::clone(&self.source),
            yaml_path: new_path,
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
            file_location: Rc::clone(&self.file_location),
            source: Rc::clone(&self.source),
            yaml_path: new_path,
        }
    }
    
    
    
    /// Find the position of a tag within the current YAML path context
    /// Uses tree-sitter for precise location finding with manual fallback
    pub fn find_tag_position_in_context(&self, tag_name: &str) -> Option<Position> {
        // Try tree-sitter first for most accurate results
        let tree_sitter_finder = TreeSitterLocationFinder::new();
        if let Some(position) = tree_sitter_finder.find_tag_position_in_context(&self.source, &self.yaml_path, tag_name) {
            return Some(position);
        }
        
        // Fallback to manual approach if tree-sitter fails
        let manual_finder = ManualLocationFinder;
        manual_finder.find_tag_position_in_context(&self.source, &self.yaml_path, tag_name)
    }
    
    
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
        // Check that we have the required "in" key (iidy-js flat format)
        let in_key = Value::String("in".to_string());
        if !map.contains_key(&in_key) {
            return Err(anyhow!("!$let tag must have an 'in' field"));
        }
        
        let expression_val = map.get(&in_key).unwrap().clone(); // Safe due to check above

        // Parse variable bindings from all keys except "in"
        let mut bindings = Vec::new();
        for (key, value) in map {
            if let Value::String(var_name) = key {
                if var_name != "in" {
                    let var_context = context.with_path(&var_name);
                    let var_value = convert_value_to_ast(value.clone(), &var_context)?;
                    bindings.push((var_name.clone(), var_value));
                }
            }
        }

        let expression = Box::new(convert_value_to_ast(expression_val, &context.with_path("in"))?);

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

/// Parse !$split tag (expects [delimiter, string] format like iidy-js)
fn parse_split_tag(value: Value, context: &ParseContext) -> Result<YamlAst> {
    if let Value::Sequence(seq) = value {
        if seq.len() != 2 {
            return Err(anyhow!("Split tag must be a sequence with two elements: [delimiter, string]"));
        }

        let delimiter = Box::new(convert_value_to_ast(seq[0].clone(), &context.with_array_index(0))?);
        let string = Box::new(convert_value_to_ast(seq[1].clone(), &context.with_array_index(1))?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Split(
            SplitTag { delimiter, string },
        )))
    } else {
        Err(anyhow!("Split tag must be a sequence with format [delimiter, string]"))
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
