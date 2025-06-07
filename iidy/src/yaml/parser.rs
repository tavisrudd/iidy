//! YAML parser with custom tag support
//! 
//! Implements parsing of YAML documents with iidy's custom preprocessing tags

use anyhow::{anyhow, Result};
use serde_yaml::{Mapping, Sequence, Value};

use crate::yaml::ast::*;
use std::collections::HashSet;

/// Validate that a mapping has exactly the required keys and optionally allowed keys, with no extras
/// Also provides helpful suggestions for common wrong field names
fn validate_exact_keys(
    map: &serde_yaml::Mapping,
    required_keys: &[&str],
    optional_keys: &[&str],
    tag_name: &str,
    file_path: &str,
    input: &str,
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
            
            // Try to find the line number by searching for the wrong key
            let line_number = if !input.is_empty() {
                input.lines().enumerate().find_map(|(idx, line)| {
                    if line.contains(&format!("{}:", wrong_key)) {
                        Some(idx + 1)
                    } else {
                        None
                    }
                }).unwrap_or(0)
            } else {
                0
            };
            
            let location = format_location(file_path, line_number);
            let suggestion = format!("use '{}' instead of '{}' in {} tags", correct_key, wrong_key, tag_name);
            
            return Err(tag_parsing_error(tag_name, &format!("'{}' should be '{}'", wrong_key, correct_key), &location, Some(&suggestion)));
        }
    }
    
    // Check for missing required keys
    let missing: Vec<String> = required_set.difference(&provided_keys).cloned().collect();
    if !missing.is_empty() {
        use crate::yaml::error_wrapper::missing_required_field_error;
        
        // Try to find the line number by searching for the tag
        let line_number = find_tag_line_number(tag_name, input);
        let location = format_location(file_path, line_number);
        
        return Err(missing_required_field_error(
            tag_name,
            &missing[0], // Show the first missing field
            &location,
            "<parsing>",
            required_keys.iter().map(|s| s.to_string()).collect()
        ));
    }
    
    // Check for extra keys
    let extra: Vec<String> = provided_keys.difference(&all_valid_keys).cloned().collect();
    if !extra.is_empty() {
        use crate::yaml::error_wrapper::tag_parsing_error;
        
        let line_number = find_tag_line_number(tag_name, input);
        let location = format_location(file_path, line_number);
        
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

/// Helper to find the line number where a tag appears
fn find_tag_line_number(tag_name: &str, input: &str) -> usize {
    if !input.is_empty() {
        input.lines().enumerate().find_map(|(idx, line)| {
            if line.contains(tag_name) {
                Some(idx + 1)
            } else {
                None
            }
        }).unwrap_or(0)
    } else {
        0
    }
}

/// Helper to format location string
fn format_location(file_path: &str, line_number: usize) -> String {
    if line_number > 0 {
        format!("{}:{}", file_path, line_number)
    } else {
        file_path.to_string()
    }
}

/// Parse YAML text with support for custom preprocessing tags
pub fn parse_yaml_with_custom_tags(input: &str) -> Result<YamlAst> {
    parse_yaml_with_custom_tags_from_file(input, "input.yaml")
}

/// Parse YAML text with file context for better error reporting  
pub fn parse_yaml_with_custom_tags_from_file(input: &str, file_path: &str) -> Result<YamlAst> {
    let value: Value = serde_yaml::from_str(input)
        .map_err(|e| crate::yaml::error_wrapper::yaml_syntax_error(e, file_path, input))?;
    convert_value_to_ast(value, file_path, input)
}

/// Convert a serde_yaml::Value to our custom AST
fn convert_value_to_ast(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    match value {
        Value::Null => Ok(YamlAst::Null),
        Value::Bool(b) => Ok(YamlAst::Bool(b)),
        Value::Number(n) => Ok(YamlAst::Number(n)),
        Value::String(s) => Ok(YamlAst::String(s)),
        Value::Sequence(seq) => convert_sequence_to_ast(seq, file_path, input),
        Value::Mapping(map) => convert_mapping_to_ast(map, file_path, input),
        Value::Tagged(tagged) => parse_tagged_value(*tagged, file_path, input),
    }
}

/// Convert a YAML sequence to AST
fn convert_sequence_to_ast(seq: Sequence, file_path: &str, input: &str) -> Result<YamlAst> {
    let mut ast_seq = Vec::new();
    for item in seq {
        ast_seq.push(convert_value_to_ast(item, file_path, input)?);
    }
    Ok(YamlAst::Sequence(ast_seq))
}

/// Convert a YAML mapping to AST
fn convert_mapping_to_ast(map: Mapping, file_path: &str, input: &str) -> Result<YamlAst> {
    // Check for special preprocessing keys like $imports, $defs
    if let Some(preprocessing_tag) = check_for_preprocessing_keys(&map)? {
        return Ok(YamlAst::PreprocessingTag(preprocessing_tag));
    }

    // Regular mapping
    let mut ast_map = Vec::new();
    for (key, value) in map {
        let key_ast = convert_value_to_ast(key, file_path, input)?;
        let value_ast = convert_value_to_ast(value, file_path, input)?;
        ast_map.push((key_ast, value_ast));
    }
    Ok(YamlAst::Mapping(ast_map))
}

/// Parse a tagged YAML value (handles !$ tags)
fn parse_tagged_value(tagged: serde_yaml::value::TaggedValue, file_path: &str, input: &str) -> Result<YamlAst> {
    let tag = tagged.tag.to_string();
    let value = tagged.value;

    match tag.as_str() {
        "!$" | "!$include" => parse_include_tag(value, file_path, input),
        "!$if" => parse_if_tag(value, file_path, input),
        "!$map" => parse_map_tag(value, file_path, input),
        "!$merge" => parse_merge_tag(value, file_path, input),
        "!$concat" => parse_concat_tag(value, file_path, input),
        "!$let" => parse_let_tag(value, file_path, input),
        "!$eq" => parse_eq_tag(value, file_path, input),
        "!$not" => parse_not_tag(value, file_path, input),
        "!$split" => parse_split_tag(value, file_path, input),
        "!$join" => parse_join_tag(value, file_path, input),
        "!$concatMap" => parse_concat_map_tag(value, file_path, input),
        "!$mergeMap" => parse_merge_map_tag(value, file_path, input),
        "!$mapListToHash" => parse_map_list_to_hash_tag(value, file_path, input),
        "!$mapValues" => parse_map_values_tag(value, file_path, input),
        "!$groupBy" => parse_group_by_tag(value, file_path, input),
        "!$fromPairs" => parse_from_pairs_tag(value, file_path, input),
        "!$toYamlString" => parse_to_yaml_string_tag(value, file_path, input),
        "!$parseYaml" => parse_parse_yaml_tag(value, file_path, input),
        "!$toJsonString" => parse_to_json_string_tag(value, file_path, input),
        "!$parseJson" => parse_parse_json_tag(value, file_path, input),
        "!$escape" => parse_escape_tag(value, file_path, input),
        _ => {
            // Check for unknown iidy preprocessing tags (likely typos) with context
            if tag.starts_with("!$") {
                {
                    use crate::yaml::error_wrapper::tag_parsing_error;
                    
                    // Try to find the line number by searching for the tag
                    let line_number = if !input.is_empty() {
                        input.lines().enumerate().find_map(|(idx, line)| {
                            if line.contains(&tag) {
                                Some(idx + 1)
                            } else {
                                None
                            }
                        }).unwrap_or(0)
                    } else {
                        0
                    };
                    
                    let location = if line_number > 0 {
                        format!("{}:{}", file_path, line_number)
                    } else {
                        file_path.to_string()
                    };
                    
                    return Err(tag_parsing_error("unknown tag", &format!("'{}' is not a valid iidy tag", tag), &location, Some("check tag spelling or see documentation for valid tags")));
                }
            }
            
            // For other tags, reconstruct the tagged value and use the original parser
            let reconstructed = serde_yaml::value::TaggedValue {
                tag: tagged.tag,
                value,
            };
            parse_non_iidy_tagged_value(reconstructed, file_path, input)
        }
    }
}

fn parse_non_iidy_tagged_value(tagged: serde_yaml::value::TaggedValue, file_path: &str, input: &str) -> Result<YamlAst> {
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
    
    let value = convert_value_to_ast(actual_value, file_path, input)?;
    Ok(YamlAst::UnknownYamlTag(UnknownTag { tag: tag_name.to_string(), value: Box::new(value) }))
}

/// Check if a mapping contains special preprocessing keys
fn check_for_preprocessing_keys(_map: &Mapping) -> Result<Option<PreprocessingTag>> {
    // For now, we'll focus on tagged values
    // Note: $imports, $defs are handled in the main preprocessing module, not in the parser
    Ok(None)
}

/// Parse !$ or !$include tag
fn parse_include_tag(value: Value, _file_path: &str, _input: &str) -> Result<YamlAst> {
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
fn parse_if_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["test", "then"], &["else"], "!$if", file_path, input)?;
        
        let condition_val = map.get(&Value::String("test".to_string())).unwrap(); // Safe due to validation
        let then_val = map.get(&Value::String("then".to_string())).unwrap(); // Safe due to validation
        let else_val = map.get(&Value::String("else".to_string()));

        let condition = Box::new(convert_value_to_ast(condition_val.clone(), file_path, input)?);
        let then_value = Box::new(convert_value_to_ast(then_val.clone(), file_path, input)?);
        let else_value = if let Some(else_val) = else_val {
            Some(Box::new(convert_value_to_ast(else_val.clone(), file_path, input)?))
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
fn parse_map_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var", "filter"], "!$map", file_path, input)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");
        
        // Optional filter
        let filter = if let Some(filter_val) = map.get(&Value::String("filter".to_string())) {
            Some(Box::new(convert_value_to_ast(filter_val.clone(), file_path, input)?))
        } else {
            None
        };

        let items = Box::new(convert_value_to_ast(items_val.clone(), file_path, input)?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), file_path, input)?);

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
fn parse_merge_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    match value {
        Value::Sequence(seq) => {
            let mut sources = Vec::new();
            for item in seq {
                sources.push(convert_value_to_ast(item, file_path, input)?);
            }
            Ok(YamlAst::PreprocessingTag(PreprocessingTag::Merge(
                MergeTag { sources },
            )))
        }
        _ => Err(anyhow!("Merge tag must be a sequence")),
    }
}

/// Parse !$concat tag
fn parse_concat_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    match value {
        Value::Sequence(seq) => {
            let mut sources = Vec::new();
            for item in seq {
                sources.push(convert_value_to_ast(item, file_path, input)?);
            }
            Ok(YamlAst::PreprocessingTag(PreprocessingTag::Concat(
                ConcatTag { sources },
            )))
        }
        _ => Err(anyhow!("Concat tag must be a sequence")),
    }
}

/// Parse !$let tag
fn parse_let_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys
        validate_exact_keys(&map, &["bindings", "expression"], &[], "!$let", file_path, input)?;
        
        let bindings_val = map.get(&Value::String("bindings".to_string())).unwrap(); // Safe due to validation
        let expression_val = map.get(&Value::String("expression".to_string())).unwrap(); // Safe due to validation

        let mut bindings = Vec::new();
        if let Value::Mapping(bindings_map) = bindings_val {
            for (key, value) in bindings_map {
                if let Value::String(var_name) = key {
                    let var_value = convert_value_to_ast(value.clone(), file_path, input)?;
                    bindings.push((var_name.clone(), var_value));
                }
            }
        }

        let expression = Box::new(convert_value_to_ast(expression_val.clone(), file_path, input)?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Let(LetTag {
            bindings,
            expression,
        })))
    } else {
        Err(anyhow!("Let tag must be a mapping"))
    }
}

/// Parse !$eq tag
fn parse_eq_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Sequence(seq) = value {
        if seq.len() != 2 {
            return Err(anyhow!("Eq tag must have exactly 2 elements"));
        }
        let left = Box::new(convert_value_to_ast(seq[0].clone(), file_path, input)?);
        let right = Box::new(convert_value_to_ast(seq[1].clone(), file_path, input)?);

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Eq(EqTag {
            left,
            right,
        })))
    } else {
        Err(anyhow!("Eq tag must be a sequence of 2 elements"))
    }
}

/// Parse !$not tag
fn parse_not_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // Handle array syntax: !$not [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let expression = Box::new(convert_value_to_ast(actual_value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::Not(NotTag {
        expression,
    })))
}

/// Parse !$split tag
fn parse_split_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        let string_val = map.get(&Value::String("string".to_string()))
            .ok_or_else(|| anyhow!("Missing 'string' in split tag"))?;
        let delimiter = extract_string_field(&map, "delimiter")?;

        let string = Box::new(convert_value_to_ast(string_val.clone(), file_path, input)?);
        let delimiter = Box::new(YamlAst::String(delimiter));

        Ok(YamlAst::PreprocessingTag(PreprocessingTag::Split(
            SplitTag { delimiter, string },
        )))
    } else {
        Err(anyhow!("Split tag must be a mapping"))
    }
}

/// Parse !$join tag (expects [delimiter, array] format like iidy-js)
fn parse_join_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Sequence(seq) = value {
        if seq.len() != 2 {
            return Err(anyhow!("Join tag must be a sequence with two elements: [delimiter, array]"));
        }

        let delimiter = Box::new(convert_value_to_ast(seq[0].clone(), file_path, input)?);
        let array = Box::new(convert_value_to_ast(seq[1].clone(), file_path, input)?);

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
fn parse_concat_map_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        // Validate that we have exactly the required keys and optionally allowed keys
        validate_exact_keys(&map, &["items", "template"], &["var", "filter"], "!$concatMap", file_path, input)?;
        
        let items_val = map.get(&Value::String("items".to_string())).unwrap(); // Safe due to validation
        let template_val = map.get(&Value::String("template".to_string())).unwrap(); // Safe due to validation
        let var_name = extract_optional_string_field(&map, "var");
        
        // Optional filter
        let filter = if let Some(filter_val) = map.get(&Value::String("filter".to_string())) {
            Some(Box::new(convert_value_to_ast(filter_val.clone(), file_path, input)?))
        } else {
            None
        };

        let items = Box::new(convert_value_to_ast(items_val.clone(), file_path, input)?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), file_path, input)?);

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
fn parse_merge_map_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        let items_val = map.get(&Value::String("items".to_string()))
            .ok_or_else(|| anyhow!("Missing 'items' in mergeMap tag"))?;
        let template_val = map.get(&Value::String("template".to_string()))
            .ok_or_else(|| anyhow!("Missing 'template' in mergeMap tag"))?;
        let var_name = extract_optional_string_field(&map, "var");

        let items = Box::new(convert_value_to_ast(items_val.clone(), file_path, input)?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), file_path, input)?);

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
fn parse_map_list_to_hash_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        let items_val = map.get(&Value::String("items".to_string()))
            .ok_or_else(|| anyhow!("Missing 'items' in mapListToHash tag"))?;
        let template_val = map.get(&Value::String("template".to_string()))
            .ok_or_else(|| anyhow!("Missing 'template' in mapListToHash tag"))?;
        let var_name = extract_optional_string_field(&map, "var");
        let filter_val = map.get(&Value::String("filter".to_string()));

        let items = Box::new(convert_value_to_ast(items_val.clone(), file_path, input)?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), file_path, input)?);
        let filter = if let Some(filter_val) = filter_val {
            Some(Box::new(convert_value_to_ast(filter_val.clone(), file_path, input)?))
        } else {
            None
        };

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
fn parse_map_values_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        let items_val = map.get(&Value::String("items".to_string()))
            .ok_or_else(|| anyhow!("Missing 'items' in mapValues tag"))?;
        let template_val = map.get(&Value::String("template".to_string()))
            .ok_or_else(|| anyhow!("Missing 'template' in mapValues tag"))?;
        let var_name = extract_optional_string_field(&map, "var");

        let items = Box::new(convert_value_to_ast(items_val.clone(), file_path, input)?);
        let template = Box::new(convert_value_to_ast(template_val.clone(), file_path, input)?);

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
fn parse_group_by_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    if let Value::Mapping(map) = value {
        let items_val = map.get(&Value::String("items".to_string()))
            .ok_or_else(|| anyhow!("Missing 'items' in groupBy tag"))?;
        let key_val = map.get(&Value::String("key".to_string()))
            .ok_or_else(|| anyhow!("Missing 'key' in groupBy tag"))?;
        let var_name = extract_optional_string_field(&map, "var");
        let template_val = map.get(&Value::String("template".to_string()));

        let items = Box::new(convert_value_to_ast(items_val.clone(), file_path, input)?);
        let key = Box::new(convert_value_to_ast(key_val.clone(), file_path, input)?);
        let template = if let Some(template_val) = template_val {
            Some(Box::new(convert_value_to_ast(template_val.clone(), file_path, input)?))
        } else {
            None
        };

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
fn parse_from_pairs_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // TODO: Consider implementing single element array unwrapping trick like other tags
    // to work around YAML tag syntax restrictions when using variables
    let source = Box::new(convert_value_to_ast(value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::FromPairs(FromPairsTag {
        source,
    })))
}

/// Parse !$toYamlString tag
fn parse_to_yaml_string_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // Handle array syntax: !$toYamlString [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let data = Box::new(convert_value_to_ast(actual_value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ToYamlString(ToYamlStringTag {
        data,
    })))
}

/// Parse !$parseYaml tag
fn parse_parse_yaml_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // Handle array syntax: !$parseYaml [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let yaml_string = Box::new(convert_value_to_ast(actual_value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ParseYaml(ParseYamlTag {
        yaml_string,
    })))
}

/// Parse !$toJsonString tag
fn parse_to_json_string_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // Handle array syntax: !$toJsonString [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let data = Box::new(convert_value_to_ast(actual_value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ToJsonString(ToJsonStringTag {
        data,
    })))
}

/// Parse !$parseJson tag
fn parse_parse_json_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // Handle array syntax: !$parseJson [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let json_string = Box::new(convert_value_to_ast(actual_value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::ParseJson(ParseJsonTag {
        json_string,
    })))
}

/// Parse !$escape tag
fn parse_escape_tag(value: Value, file_path: &str, input: &str) -> Result<YamlAst> {
    // Handle array syntax: !$escape [expression] should extract the expression
    let actual_value = match value {
        Value::Sequence(seq) if seq.len() == 1 => seq.into_iter().next().unwrap(),
        other => other,
    };
    
    let content = Box::new(convert_value_to_ast(actual_value, file_path, input)?);
    Ok(YamlAst::PreprocessingTag(PreprocessingTag::Escape(EscapeTag {
        content,
    })))
}
