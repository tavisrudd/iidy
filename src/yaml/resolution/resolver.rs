//! Split-args resolver implementation
//!
//! This module provides a performance-optimized approach that separates frequently-changing
//! path tracking from the mostly-static TagContext, eliminating unnecessary cloning.
//!
//! ## Architecture
//!
//! ### Problem with Current Approach
//! - PathTracker efficiently tracks document location during resolution
//! - Most context fields (variables, input_uri, scope_context) are static during traversal
//! - Only the path changes frequently as we navigate the AST
//!
//! ### Split-Args Solution
//! - **Static Context**: Variables, input_uri, scope_context (rarely changes)
//! - **Dynamic Path**: Separate SmallVec-based path tracker (changes frequently)
//! - **Method Signature**: `resolve_ast(ast, context, path_tracker)`
//!
//! ## Performance Benefits
//! - **No context cloning**: Static context passed by reference
//! - **Efficient path tracking**: SmallVec with stack allocation for typical depths
//! - **Minimal allocations**: Only path segments are added/removed
//! - **Cache friendly**: Better memory locality with separate concerns

use anyhow::{Context, Result, anyhow};
use serde_yaml::Value;
use serde_yaml::value::{Tag, TaggedValue};
use std::collections::HashMap;

use crate::yaml::errors::cloudformation_validation_error_with_path_tracker;
use crate::yaml::errors::variable_not_found_error_with_path_tracker;
use crate::yaml::errors::wrapper::type_mismatch_error_with_path_tracker;
use crate::yaml::handlebars::interpolate_handlebars_string;
use crate::yaml::parsing::ast::*;
use crate::yaml::path_tracker::PathTracker;
use crate::yaml::resolution::context::TagContext;

/// Helper trait for extracting human-readable type strings from serde_yaml::Value
trait ValueTypeStr {
    /// Get a human-readable type string for error reporting
    fn to_type_str(&self) -> &'static str;
}

impl ValueTypeStr for Value {
    fn to_type_str(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Sequence(_) => "sequence",
            Value::Mapping(_) => "object",
            Value::Tagged(_) => "tagged value",
        }
    }
}

/// Check if an AST value is simple (no processing needed)
#[inline(always)]
fn is_simple_ast_value(ast: &YamlAst) -> bool {
    match ast {
        YamlAst::Null(_)
        | YamlAst::Bool(_, _)
        | YamlAst::Number(_, _)
        | YamlAst::PlainString(_, _) => true,
        _ => false,
    }
}

/// Check if all items in a sequence are simple values
#[inline(always)]
fn is_simple_sequence(seq: &[YamlAst]) -> bool {
    seq.iter().all(is_simple_ast_value)
}

/// Check if a mapping contains only simple string keys and simple values
/// Excludes preprocessing directive keys (starting with '$')
#[inline(always)]
fn is_simple_mapping(pairs: &[(YamlAst, YamlAst)]) -> bool {
    pairs.iter().all(|(key, value)| match key {
        YamlAst::PlainString(s, _) if !s.starts_with('$') => is_simple_ast_value(value),
        _ => false,
    })
}

/// Convert a simple AST value directly to serde_yaml::Value
/// Panics if called on non-simple AST (should be checked with is_simple_ast_value first)
#[inline(always)]
fn simple_ast_to_value(ast: &YamlAst) -> Value {
    match ast {
        YamlAst::Null(_) => Value::Null,
        YamlAst::Bool(b, _) => Value::Bool(*b),
        YamlAst::Number(n, _) => Value::Number(n.clone()),
        YamlAst::PlainString(s, _) => Value::String(s.clone()),
        _ => unreachable!("simple_ast_to_value called on non-simple AST"),
    }
}

pub trait TagResolver {
    /// Resolve an AST node to a value
    fn resolve_ast(
        &self,
        ast: &YamlAst,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;

    fn resolve_template_string(
        &self,
        template: &str,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_mapping(
        &self,
        pairs: &[(YamlAst, YamlAst)],
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;

    fn resolve_sequence(
        &self,
        items: &[YamlAst],
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;

    // Core tag resolution methods with split args
    fn resolve_preprocessing_tag(
        &self,
        tag: &PreprocessingTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_cloudformation_tag(
        &self,
        cfn_tag: &CloudFormationTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn validate_cloudformation_tag(
        &self,
        cfn_tag: &CloudFormationTag,
        resolved_value: &Value,
        context: &TagContext,
        path_tracker: &PathTracker,
    ) -> Result<()>;

    fn resolve_include(
        &self,
        tag: &IncludeTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_if(
        &self,
        tag: &IfTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_map(
        &self,
        tag: &MapTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_merge(
        &self,
        tag: &MergeTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_concat(
        &self,
        tag: &ConcatTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_let(
        &self,
        tag: &LetTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_eq(
        &self,
        tag: &EqTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_not(
        &self,
        tag: &NotTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_split(
        &self,
        tag: &SplitTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_join(
        &self,
        tag: &JoinTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;

    // Advanced transformation tags
    fn resolve_concat_map(
        &self,
        tag: &ConcatMapTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_merge_map(
        &self,
        tag: &MergeMapTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_map_list_to_hash(
        &self,
        tag: &MapListToHashTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_map_values(
        &self,
        tag: &MapValuesTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_group_by(
        &self,
        tag: &GroupByTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_from_pairs(
        &self,
        tag: &FromPairsTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;

    // String processing tags
    fn resolve_to_yaml_string(
        &self,
        tag: &ToYamlStringTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_parse_yaml(
        &self,
        tag: &ParseYamlTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_to_json_string(
        &self,
        tag: &ToJsonStringTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_parse_json(
        &self,
        tag: &ParseJsonTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
    fn resolve_escape(
        &self,
        tag: &EscapeTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value>;
}

/// Split-args tag resolver that separates static context from dynamic path tracking
#[derive(Debug)]
pub struct Resolver;

impl Resolver {
    /// Helper function to check if a value is truthy
    fn is_truthy(&self, value: &Value) -> bool {
        match value {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
            Value::Sequence(seq) => !seq.is_empty(),
            Value::Mapping(map) => !map.is_empty(),
            _ => true,
        }
    }

    /// Helper function to compare values for equality
    fn values_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                a.as_f64().unwrap_or(0.0) == b.as_f64().unwrap_or(0.0)
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Sequence(a), Value::Sequence(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| self.values_equal(x, y))
            }
            (Value::Mapping(a), Value::Mapping(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(k).map_or(false, |v2| self.values_equal(v, v2)))
            }
            _ => false,
        }
    }

    /// Extract file path from context for error reporting
    #[inline(always)]
    fn get_file_path<'a>(&self, context: &'a TagContext) -> &'a str {
        context.input_uri.as_deref().unwrap_or("unknown")
    }

    /// Create a type mismatch error with consistent formatting
    fn create_type_mismatch_error(
        &self,
        expected_type: &str,
        found_value: &Value,
        context_desc: &str,
        context: &TagContext,
        path_tracker: &PathTracker,
    ) -> anyhow::Error {
        let file_path = self.get_file_path(context);
        let found_type = found_value.to_type_str();
        type_mismatch_error_with_path_tracker(
            expected_type,
            found_type,
            context_desc,
            file_path,
            path_tracker,
        )
    }

    /// Parse path and query from include path
    fn parse_path_and_query(
        &self,
        path: &str,
        explicit_query: &Option<String>,
    ) -> (String, Option<String>) {
        if let Some(query) = explicit_query {
            return (path.to_string(), Some(query.clone()));
        }

        if let Some(query_start) = path.find('?') {
            let base_path = path[..query_start].to_string();
            let query = path[query_start + 1..].to_string();
            (base_path, Some(query))
        } else {
            (path.to_string(), None)
        }
    }

    /// Resolve dot notation path in variables (simplified version)
    /// Handles both dot notation (config.database.host) and simple bracket notation (config[environment])
    fn resolve_dot_notation_path(&self, path: &str, context: &TagContext) -> Option<Value> {
        // Handle simple bracket notation like config[environment]
        if let Some(bracket_start) = path.find('[') {
            if let Some(bracket_end) = path.find(']') {
                if bracket_start < bracket_end {
                    let root_var = &path[..bracket_start];
                    let bracket_content = &path[bracket_start + 1..bracket_end];

                    // Get the root variable
                    let root_value = context.get_variable(root_var)?;

                    // Get the bracket variable value to use as key
                    let key_value = context.get_variable(bracket_content)?;

                    // Use the key value to look up in the root mapping
                    if let Value::Mapping(map) = root_value {
                        // Convert the key value to the appropriate map key
                        let map_key = match key_value {
                            Value::String(s) => Value::String(s.clone()),
                            other => other.clone(),
                        };
                        return map.get(&map_key).cloned();
                    }
                    return None;
                }
            }
        }

        // Handle normal dot notation
        let path_segments: Vec<String> = path.split('.').map(|s| s.to_string()).collect();

        if path_segments.is_empty() {
            return None;
        }

        // Start with the root variable
        let root_var = &path_segments[0];
        let current_value = context.get_variable(root_var);
        if current_value.is_none() {
            return None;
        }
        let mut current_value = current_value.unwrap();

        // Traverse the path segments using references until the end
        for segment in &path_segments[1..] {
            match current_value {
                Value::Mapping(map) => {
                    let key = Value::String(segment.clone());
                    let next_value = map.get(&key);
                    if next_value.is_none() {
                        return None;
                    }
                    current_value = next_value.unwrap();
                }
                _ => return None, // Can't traverse further
            }
        }

        // Only clone at the final step
        Some(current_value.clone())
    }

    /// Apply query selector to a value (simplified version)
    fn apply_query_selector(&self, value: &Value, query: &str) -> Result<Value> {
        match value {
            Value::Mapping(map) => {
                if query.starts_with('.') {
                    // TODO: remove dot-prefixed query support -- it's redundant
                    // with dot notation in the path itself (e.g. config.database.host)
                    let path = &query[1..];
                    self.apply_nested_path_query(value, path)
                } else if query.contains(',') {
                    // Handle multiple property selection like "database,host"
                    let properties: Vec<&str> = query.split(',').map(|s| s.trim()).collect();
                    let mut result = serde_yaml::Mapping::with_capacity(properties.len());

                    for prop in properties {
                        if let Some(prop_value) = map.get(&Value::String(prop.to_string())) {
                            result.insert(Value::String(prop.to_string()), prop_value.clone());
                        }
                    }

                    Ok(Value::Mapping(result))
                } else {
                    // TODO: remove single-key query support -- it's redundant
                    // with dot notation in the path (e.g. config.database.host)
                    if let Some(prop_value) = map.get(&Value::String(query.to_string())) {
                        Ok(prop_value.clone())
                    } else {
                        Err(anyhow!("Property '{}' not found in mapping", query))
                    }
                }
            }
            _ => Err(anyhow!(
                "Query selectors can only be applied to mappings, found {}",
                value.to_type_str()
            )),
        }
    }

    /// Apply nested path query to a value
    fn apply_nested_path_query(&self, value: &Value, path: &str) -> Result<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current_value = value;

        for part in parts {
            if part.is_empty() {
                continue;
            }

            match current_value {
                Value::Mapping(map) => {
                    let key = Value::String(part.to_string());
                    if let Some(next_value) = map.get(&key) {
                        current_value = next_value;
                    } else {
                        return Err(anyhow!("Property '{}' not found in path", part));
                    }
                }
                _ => {
                    return Err(anyhow!(
                        "Cannot traverse path further at '{}', found {}",
                        part,
                        current_value.to_type_str()
                    ));
                }
            }
        }

        Ok(current_value.clone())
    }
    /// Process handlebars templates in strings
    fn process_string_with_handlebars(
        &self,
        s: String,
        context: &TagContext,
        path_tracker: &PathTracker,
    ) -> Result<Value> {
        // Check if string contains handlebars syntax
        if !s.contains("{{") {
            return Ok(Value::String(s));
        }

        // Convert TagContext variables from serde_yaml::Value to serde_json::Value
        let mut env_values: HashMap<String, serde_json::Value> =
            HashMap::with_capacity(context.variables.len());

        for (key, yaml_value) in &context.variables {
            // Convert serde_yaml::Value to serde_json::Value
            let json_value = yaml_to_json_value(yaml_value)?;
            env_values.insert(key.clone(), json_value);
        }

        // Interpolate handlebars template with enhanced error handling
        match interpolate_handlebars_string(&s, &env_values, "split_args_resolver") {
            Ok(interpolated) => Ok(Value::String(interpolated)),
            Err(e) => {
                let file_path = context.input_uri.as_deref().unwrap_or("unknown location");
                let error_msg = e.to_string();

                if let Some(var_name) = parse_variable_name_from_handlebars_error(&error_msg) {
                    let location = find_template_variable_location(file_path, var_name);
                    let available_vars: Vec<String> = env_values.keys().cloned().collect();
                    return Err(variable_not_found_error_with_path_tracker(
                        var_name,
                        &location,
                        path_tracker,
                        available_vars,
                    ));
                }

                Err(anyhow!(
                    "Failed to process handlebars template in {}: {}",
                    file_path,
                    s
                ))
                .with_context(|| format!("Handlebars processing failed: {}", e))
                .with_context(|| {
                    format!(
                        "processing template at path /{}",
                        path_tracker.segments().join("/")
                    )
                })
            }
        }
    }
    /// Convert AST to Value without any preprocessing (for escape tag)
    fn ast_to_value_without_preprocessing(&self, ast: &YamlAst) -> Result<Value> {
        match ast {
            YamlAst::Null(_) => Ok(Value::Null),
            YamlAst::Bool(b, _) => Ok(Value::Bool(*b)),
            YamlAst::Number(n, _) => Ok(Value::Number(n.clone())),
            YamlAst::PlainString(s, _) | YamlAst::TemplatedString(s, _) => {
                Ok(Value::String(s.clone()))
            }
            YamlAst::Sequence(seq, _) => {
                let mut result = Vec::with_capacity(seq.len());
                for item in seq {
                    result.push(self.ast_to_value_without_preprocessing(item)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(pairs, _) => {
                let mut result = serde_yaml::Mapping::with_capacity(pairs.len());
                for (key, value) in pairs {
                    let key_val = self.ast_to_value_without_preprocessing(key)?;
                    let value_val = self.ast_to_value_without_preprocessing(value)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(_, _) => {
                // Escaped preprocessing tags should be converted to strings
                Ok(Value::String(format!("!${}", "escaped_tag")))
            }
            YamlAst::CloudFormationTag(cfn_tag, _) => {
                // Escaped CloudFormation tags should preserve their structure
                let mut result = serde_yaml::Mapping::with_capacity(1);
                let tag_name = format!("!{}", cfn_tag.tag_name());
                let inner_val = self.ast_to_value_without_preprocessing(cfn_tag.inner_value())?;
                result.insert(Value::String(tag_name), inner_val);
                Ok(Value::Mapping(result))
            }
            YamlAst::UnknownYamlTag(unknown, _) => {
                self.ast_to_value_without_preprocessing(&unknown.value)
            }
            YamlAst::ImportedDocument(doc, _) => {
                self.ast_to_value_without_preprocessing(&doc.content)
            }
        }
    }

    /// Core map logic shared by resolve_map, resolve_concat_map, and resolve_merge_map
    fn resolve_map_items(
        &self,
        items: &YamlAst,
        template: &YamlAst,
        var: Option<&str>,
        filter: Option<&YamlAst>,
        tag_name: &str,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let items_result = self.resolve_ast(items, context, path_tracker)?;

        match items_result {
            Value::Sequence(seq) => {
                let mut result = Vec::with_capacity(seq.len());
                let var_name = var.unwrap_or("item");

                for (idx, item) in seq.into_iter().enumerate() {
                    let mut item_bindings = HashMap::with_capacity(2);
                    item_bindings.insert(var_name.to_string(), item);
                    item_bindings.insert(
                        format!("{}Idx", var_name),
                        Value::Number(serde_yaml::Number::from(idx)),
                    );
                    let item_context = context.with_bindings_ref(&item_bindings);

                    if let Some(filter) = filter {
                        let filter_result =
                            self.resolve_ast(filter, &item_context, path_tracker)?;
                        if !self.is_truthy(&filter_result) {
                            continue;
                        }
                    }

                    let transformed = self.resolve_ast(template, &item_context, path_tracker)?;
                    result.push(transformed);
                }

                Ok(Value::Sequence(result))
            }
            _ => Err(self.create_type_mismatch_error(
                "sequence",
                &items_result,
                &format!("{} items field", tag_name),
                context,
                path_tracker,
            )),
        }
    }

    /// Resolve a mapping at the CFN Resources level, expanding custom resource types.
    fn resolve_resources_mapping(
        &self,
        pairs: &[(YamlAst, YamlAst)],
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        use crate::yaml::custom_resources::expansion::expand_custom_resource;

        let mut result = serde_yaml::Mapping::with_capacity(pairs.len());

        for (key_ast, value_ast) in pairs {
            let key_value = self.resolve_ast(key_ast, context, path_tracker)?;

            let is_preprocessing_key = key_value
                .as_str()
                .map_or(false, |s| s.starts_with('$'));
            if is_preprocessing_key {
                let key_str = key_value.as_str().unwrap();
                if matches!(key_str, "$imports" | "$defs" | "$envValues" | "$params") {
                    continue;
                }
            }

            let path_segment = match &key_value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => format!("{:?}", key_value),
            };

            path_tracker.push(&path_segment);
            let resolved_value = self.resolve_ast(value_ast, context, path_tracker)?;
            path_tracker.pop();

            // Check if this resource uses a custom resource type
            let resource_type = resolved_value
                .as_mapping()
                .and_then(|m| m.get(&Value::String("Type".into())))
                .and_then(|v| v.as_str());

            if let Some(type_name) = resource_type {
                if let Some(template_info) = context.custom_template_defs.get(type_name) {
                    let key_str = key_value.as_str().unwrap_or(&path_segment);
                    let expanded = expand_custom_resource(
                        key_str,
                        &resolved_value,
                        template_info,
                        context,
                    )?;
                    for (res_name, res_value) in expanded {
                        result.insert(Value::String(res_name), res_value);
                    }
                    continue;
                }
            }

            result.insert(key_value, resolved_value);
        }

        Ok(Value::Mapping(result))
    }
}

impl TagResolver for Resolver {
    /// Resolve an AST node with split arguments for better performance
    fn resolve_ast(
        &self,
        ast: &YamlAst,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        match ast {
            // Scalars - direct conversion
            YamlAst::Null(_) => Ok(Value::Null),
            YamlAst::Bool(b, _) => Ok(Value::Bool(*b)),
            YamlAst::Number(n, _) => Ok(Value::Number(n.clone())),
            YamlAst::PlainString(s, _) => Ok(Value::String(s.clone())),

            // Templated strings - need variable resolution
            YamlAst::TemplatedString(template, _) => {
                self.resolve_template_string(template, context, path_tracker)
            }

            // Composite types
            YamlAst::Mapping(pairs, _) => self.resolve_mapping(pairs, context, path_tracker),
            YamlAst::Sequence(items, _) => self.resolve_sequence(items, context, path_tracker),

            YamlAst::PreprocessingTag(tag, _) => {
                self.resolve_preprocessing_tag(tag, context, path_tracker)
            }

            YamlAst::CloudFormationTag(cfn_tag, _) => {
                self.resolve_cloudformation_tag(cfn_tag, context, path_tracker)
            }

            YamlAst::UnknownYamlTag(tag, _) => {
                // Convert unknown tags to strings for now
                let resolved_value = self.resolve_ast(&tag.value, context, path_tracker)?;
                let tagged_value = TaggedValue {
                    tag: Tag::new(&tag.tag),
                    value: resolved_value,
                };
                Ok(Value::Tagged(Box::new(tagged_value)))
            }

            YamlAst::ImportedDocument(doc, _) => {
                // Process the imported document content
                self.resolve_ast(&doc.content, context, path_tracker)
            }
        }
    }

    /// Resolve a mapping with efficient path tracking
    fn resolve_mapping(
        &self,
        pairs: &[(YamlAst, YamlAst)],
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let mut result = serde_yaml::Mapping::with_capacity(pairs.len());

        if is_simple_mapping(pairs) {
            // Fast path: simple key-value pairs with no processing needed
            for (key, value) in pairs {
                let key_val = simple_ast_to_value(key);
                let value_val = simple_ast_to_value(value);
                result.insert(key_val, value_val);
            }
        } else {
            // Custom resource expansion at the CFN Resources level
            if !context.custom_template_defs.is_empty()
                && path_tracker.segments() == ["Resources"]
            {
                return self.resolve_resources_mapping(pairs, context, path_tracker);
            }

            // Complex path: need full processing with path tracking
            for (key_ast, value_ast) in pairs {
                // Resolve key
                let key_value = self.resolve_ast(key_ast, context, path_tracker)?;

                // Check for YAML 1.1 merge keys which are not supported in YAML 1.2
                if let Value::String(key_str) = &key_value {
                    if key_str == "<<" {
                        let location_info = if let Some(input_uri) = &context.input_uri {
                            format!("in file '{}'", input_uri)
                        } else {
                            context
                                .input_uri
                                .as_deref()
                                .map(|loc| format!("in '{}'", loc))
                                .unwrap_or_else(|| "in unknown location".to_string())
                        };
                        let yaml_path = format!("/{}", path_tracker.segments().join("/"));
                        let path_info = if !yaml_path.is_empty() && yaml_path != "/" {
                            format!(" at path '{}'", yaml_path)
                        } else {
                            String::new()
                        };
                        return Err(anyhow!(
                            "YAML merge keys ('<<') are not supported in YAML 1.2 {}{}\n\
                            Consider using iidy's !$merge tag instead:\n\
                              combined_config: !$merge\n\
                                - *base_config\n\
                                - additional_key: additional_value",
                            location_info,
                            path_info
                        ));
                    }
                }

                // Handle special preprocessing keys (only for string keys)
                let is_preprocessing_key = match &key_value {
                    Value::String(s) => s.starts_with('$'),
                    _ => false,
                };

                if is_preprocessing_key {
                    let key_str = key_value.as_str().unwrap(); // Safe because we checked above
                    if matches!(key_str, "$imports" | "$defs" | "$envValues" | "$params") {
                        continue;
                    }
                }

                // Create path segment for tracking (convert key to string representation)
                let path_segment = match &key_value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", key_value),
                };

                // Resolve value with path tracking
                path_tracker.push(&path_segment);
                let resolved_value = self.resolve_ast(value_ast, context, path_tracker)?;
                path_tracker.pop();

                // Add to result (use original key value, not just string)
                result.insert(key_value, resolved_value);
            }
        }

        Ok(Value::Mapping(result))
    }

    /// Resolve a sequence with efficient path tracking
    fn resolve_sequence(
        &self,
        items: &[YamlAst],
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let mut result = Vec::with_capacity(items.len());

        if is_simple_sequence(items) {
            // Fast path: convert simple values directly without path tracking
            for item in items {
                result.push(simple_ast_to_value(item));
            }
        } else {
            // Complex path: need full processing with path tracking
            for (index, item) in items.iter().enumerate() {
                path_tracker.push(&format!("[{}]", index));
                let resolved_item = self.resolve_ast(item, context, path_tracker)?;
                path_tracker.pop();

                result.push(resolved_item);
            }
        }

        Ok(Value::Sequence(result))
    }

    /// Resolve a templated string with handlebars processing
    fn resolve_template_string(
        &self,
        template: &str,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        self.process_string_with_handlebars(template.to_string(), context, path_tracker)
    }

    /// Resolve a preprocessing tag
    fn resolve_preprocessing_tag(
        &self,
        tag: &PreprocessingTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        match tag {
            PreprocessingTag::Include(include_tag) => {
                self.resolve_include(include_tag, context, path_tracker)
            }
            PreprocessingTag::If(if_tag) => self.resolve_if(if_tag, context, path_tracker),
            PreprocessingTag::Let(let_tag) => self.resolve_let(let_tag, context, path_tracker),
            PreprocessingTag::Map(map_tag) => self.resolve_map(map_tag, context, path_tracker),
            PreprocessingTag::Merge(merge_tag) => {
                self.resolve_merge(merge_tag, context, path_tracker)
            }
            PreprocessingTag::Concat(concat_tag) => {
                self.resolve_concat(concat_tag, context, path_tracker)
            }
            PreprocessingTag::Eq(eq_tag) => self.resolve_eq(eq_tag, context, path_tracker),
            PreprocessingTag::Not(not_tag) => self.resolve_not(not_tag, context, path_tracker),
            PreprocessingTag::Split(split_tag) => {
                self.resolve_split(split_tag, context, path_tracker)
            }
            PreprocessingTag::Join(join_tag) => self.resolve_join(join_tag, context, path_tracker),
            PreprocessingTag::MapValues(map_values_tag) => {
                self.resolve_map_values(map_values_tag, context, path_tracker)
            }
            PreprocessingTag::ConcatMap(concat_map_tag) => {
                self.resolve_concat_map(concat_map_tag, context, path_tracker)
            }
            PreprocessingTag::MergeMap(merge_map_tag) => {
                self.resolve_merge_map(merge_map_tag, context, path_tracker)
            }
            PreprocessingTag::MapListToHash(map_list_to_hash_tag) => {
                self.resolve_map_list_to_hash(map_list_to_hash_tag, context, path_tracker)
            }
            PreprocessingTag::GroupBy(group_by_tag) => {
                self.resolve_group_by(group_by_tag, context, path_tracker)
            }
            PreprocessingTag::FromPairs(from_pairs_tag) => {
                self.resolve_from_pairs(from_pairs_tag, context, path_tracker)
            }
            PreprocessingTag::ToJsonString(to_json_tag) => {
                self.resolve_to_json_string(to_json_tag, context, path_tracker)
            }
            PreprocessingTag::ToYamlString(to_yaml_tag) => {
                self.resolve_to_yaml_string(to_yaml_tag, context, path_tracker)
            }
            PreprocessingTag::ParseJson(parse_json_tag) => {
                self.resolve_parse_json(parse_json_tag, context, path_tracker)
            }
            PreprocessingTag::ParseYaml(parse_yaml_tag) => {
                self.resolve_parse_yaml(parse_yaml_tag, context, path_tracker)
            }
            PreprocessingTag::Escape(escape_tag) => {
                self.resolve_escape(escape_tag, context, path_tracker)
            }
        }
    }

    /// Resolve an include tag  
    fn resolve_include(
        &self,
        tag: &IncludeTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let path = &tag.path;

        // Parse path and query - simplified version
        let (base_path, query) = self.parse_path_and_query(path, &tag.query);

        // Try to resolve the variable from the environment
        if let Some(mut value) = self.resolve_dot_notation_path(&base_path, context) {
            // Apply query selector if present
            if let Some(query_str) = query {
                value = self.apply_query_selector(&value, &query_str)?;
            }
            return Ok(value);
        }

        // Variable not found - provide enhanced error context (matching main resolver)
        // Check if the root variable exists to give a better error message
        let root_var = base_path
            .split('.')
            .next()
            .unwrap_or(&base_path)
            .split('[')
            .next()
            .unwrap_or(&base_path);

        // Get file path
        let file_path = if let Some(input_uri) = &context.input_uri {
            input_uri.clone()
        } else {
            context
                .input_uri
                .as_deref()
                .unwrap_or("unknown location")
                .to_string()
        };

        // Get available variables
        let available_vars: Vec<String> = context.variables.keys().cloned().collect();

        // Check if root variable exists
        if let Some(_root_value) = context.get_variable(root_var) {
            // Root variable exists, but path resolution failed - this means a property doesn't exist
            let property_path = if base_path.contains('.') {
                base_path.split('.').skip(1).collect::<Vec<_>>().join(".")
            } else if base_path.contains('[') {
                base_path.split('[').skip(1).collect::<Vec<_>>().join("[")
            } else {
                base_path.to_string()
            };

            // Note: blocking read in a sync method called from async context. Acceptable because
            // this is an error-only path reading a local file already loaded by the resolver.
            let location = if let Ok(content) = std::fs::read_to_string(&file_path) {
                let line_number = content
                    .lines()
                    .enumerate()
                    .find_map(|(idx, line)| {
                        let patterns = [
                            format!("!$ {}", base_path),
                            format!("!$include {}", base_path),
                            format!("!$include: {}", base_path),
                            format!("!$include\\n  path: {}", base_path),
                            format!("path: {}", base_path),
                            base_path.clone(),
                        ];

                        for pattern in &patterns {
                            if line.contains(pattern) {
                                return Some(idx + 1);
                            }
                        }
                        None
                    })
                    .unwrap_or(0);

                if line_number > 0 {
                    format!("{}:{}", file_path, line_number)
                } else {
                    file_path.clone()
                }
            } else {
                file_path.clone()
            };

            use crate::yaml::errors::variable_not_found_error_with_path_tracker;
            return Err(variable_not_found_error_with_path_tracker(
                &format!("{}.{}", root_var, property_path),
                &location,
                path_tracker,
                available_vars,
            ));
        } else {
            // Note: blocking read in a sync method called from async context. Acceptable because
            // this is an error-only path reading a local file already loaded by the resolver.
            let location = if let Ok(content) = std::fs::read_to_string(&file_path) {
                let line_number = content
                    .lines()
                    .enumerate()
                    .find_map(|(idx, line)| {
                        let patterns = [
                            format!("!$ {}", root_var),         // Standard pattern: !$ var
                            format!("!$include {}", root_var),  // Explicit include tag
                            format!("!$include: {}", root_var), // Include with colon
                            format!("path: {}", root_var),      // Just the path part
                            root_var.to_string(),               // Direct variable reference
                        ];

                        for pattern in &patterns {
                            if line.contains(pattern) {
                                return Some(idx + 1);
                            }
                        }
                        None
                    })
                    .unwrap_or(0);

                if line_number > 0 {
                    format!("{}:{}", file_path, line_number)
                } else {
                    file_path.clone()
                }
            } else {
                file_path.clone()
            };

            use crate::yaml::errors::variable_not_found_error_with_path_tracker;
            Err(variable_not_found_error_with_path_tracker(
                root_var,
                &location,
                path_tracker,
                available_vars,
            ))
        }
    }

    /// Resolve an if tag
    fn resolve_if(
        &self,
        tag: &IfTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let condition_result = self.resolve_ast(&tag.test, context, path_tracker)?;

        if self.is_truthy(&condition_result) {
            self.resolve_ast(&tag.then_value, context, path_tracker)
        } else if let Some(ref else_value) = tag.else_value {
            self.resolve_ast(else_value, context, path_tracker)
        } else {
            Ok(Value::Null)
        }
    }

    /// Resolve a let tag
    fn resolve_let(
        &self,
        tag: &LetTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let mut bindings = HashMap::with_capacity(tag.bindings.len());

        // Resolve all variable bindings
        for (var_name, var_expr) in &tag.bindings {
            let var_value = self.resolve_ast(var_expr, context, path_tracker)?;
            bindings.insert(var_name.clone(), var_value);
        }

        // Create new context with bindings and resolve expression
        let new_context = context.with_bindings_ref(&bindings);
        self.resolve_ast(&tag.expression, &new_context, path_tracker)
    }

    /// Resolve a map tag
    fn resolve_map(
        &self,
        tag: &MapTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        self.resolve_map_items(
            &tag.items,
            &tag.template,
            tag.var.as_deref(),
            tag.filter.as_deref(),
            "!$map",
            context,
            path_tracker,
        )
    }

    /// Resolve a merge tag
    fn resolve_merge(
        &self,
        tag: &MergeTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        // Pre-allocate with estimated capacity based on number of sources
        let mut result = serde_yaml::Mapping::with_capacity(tag.sources.len() * 4);

        for source in &tag.sources {
            let source_result = self.resolve_ast(source, context, path_tracker)?;
            match source_result {
                Value::Mapping(map) => {
                    result.extend(map);
                }
                _ => {
                    return Err(self.create_type_mismatch_error(
                        "object",
                        &source_result,
                        "!$merge source argument",
                        context,
                        path_tracker,
                    ));
                }
            }
        }

        Ok(Value::Mapping(result))
    }

    /// Resolve a concat tag
    fn resolve_concat(
        &self,
        tag: &ConcatTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        // Pre-allocate with estimated capacity
        let mut result = Vec::with_capacity(tag.sources.len() * 2);

        for source in &tag.sources {
            let source_result = self.resolve_ast(source, context, path_tracker)?;
            match source_result {
                Value::Sequence(mut seq) => {
                    result.append(&mut seq);
                }
                other => {
                    // Single item, add it to the result
                    result.push(other);
                }
            }
        }

        Ok(Value::Sequence(result))
    }

    /// Resolve an eq tag
    fn resolve_eq(
        &self,
        tag: &EqTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        // Use local implementation instead of private function

        let left = self.resolve_ast(&tag.left, context, path_tracker)?;
        let right = self.resolve_ast(&tag.right, context, path_tracker)?;

        let is_equal = self.values_equal(&left, &right);
        Ok(Value::Bool(is_equal))
    }

    /// Resolve a not tag
    fn resolve_not(
        &self,
        tag: &NotTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let expr_result = self.resolve_ast(&tag.expression, context, path_tracker)?;
        Ok(Value::Bool(!self.is_truthy(&expr_result)))
    }

    /// Resolve a split tag
    fn resolve_split(
        &self,
        tag: &SplitTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let delimiter_result = self.resolve_ast(&tag.delimiter, context, path_tracker)?;
        let string_result = self.resolve_ast(&tag.string, context, path_tracker)?;

        match (&delimiter_result, &string_result) {
            (Value::String(delimiter), Value::String(s)) => {
                let parts: Vec<Value> = s
                    .split(delimiter)
                    .map(|part| Value::String(part.to_string()))
                    .collect();
                Ok(Value::Sequence(parts))
            }
            _ => {
                let file_path = context.input_uri.as_deref().unwrap_or("unknown");

                // Determine which argument has the wrong type
                let (expected_type, found_type, context_desc) =
                    match (&delimiter_result, &string_result) {
                        (Value::String(_), _) => {
                            // Delimiter is correct, string is wrong
                            let found_type = string_result.to_type_str();
                            ("string", found_type, "!$split string argument")
                        }
                        (_, Value::String(_)) => {
                            // String is correct, delimiter is wrong
                            let found_type = delimiter_result.to_type_str();
                            ("string", found_type, "!$split delimiter argument")
                        }
                        (_, _) => {
                            // Both are wrong, report delimiter first
                            let found_type = delimiter_result.to_type_str();
                            ("string", found_type, "!$split delimiter argument")
                        }
                    };

                Err(type_mismatch_error_with_path_tracker(
                    expected_type,
                    found_type,
                    context_desc,
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a join tag
    fn resolve_join(
        &self,
        tag: &JoinTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let delimiter_result = self.resolve_ast(&tag.delimiter, context, path_tracker)?;
        let array_result = self.resolve_ast(&tag.array, context, path_tracker)?;

        // Extract delimiter as string
        let delimiter_str = match delimiter_result {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => {
                return Err(self.create_type_mismatch_error(
                    "string",
                    &delimiter_result,
                    "!$join delimiter argument",
                    context,
                    path_tracker,
                ));
            }
        };

        match array_result {
            Value::Sequence(seq) => {
                let strings: Result<Vec<String>, _> = seq
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => Ok(s),
                        Value::Number(n) => Ok(n.to_string()),
                        Value::Bool(b) => Ok(b.to_string()),
                        _ => {
                            let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                            let found_type = v.to_type_str();

                            Err(type_mismatch_error_with_path_tracker(
                                "string",
                                found_type,
                                "!$join sequence item",
                                file_path,
                                path_tracker,
                            ))
                        }
                    })
                    .collect();

                let joined = strings?.join(&delimiter_str);
                Ok(Value::String(joined))
            }
            _ => Err(self.create_type_mismatch_error(
                "sequence",
                &array_result,
                "!$join sequence argument",
                context,
                path_tracker,
            )),
        }
    }

    /// Resolve a map values tag
    fn resolve_map_values(
        &self,
        tag: &MapValuesTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context, path_tracker)?;

        match items_result {
            Value::Mapping(map) => {
                let mut result = serde_yaml::Mapping::with_capacity(map.len());
                let var_name = tag.var.as_deref().unwrap_or("item");

                for (key, value) in map {
                    // Create bindings for key and value
                    let mut item_bindings = HashMap::with_capacity(2);
                    let item_obj = {
                        let mut obj = serde_yaml::Mapping::with_capacity(2);
                        obj.insert(Value::String("key".to_string()), key.clone());
                        obj.insert(Value::String("value".to_string()), value);
                        Value::Mapping(obj)
                    };
                    item_bindings.insert(var_name.to_string(), item_obj);
                    let item_context = context.with_bindings_ref(&item_bindings);

                    let transformed =
                        self.resolve_ast(&tag.template, &item_context, path_tracker)?;
                    result.insert(key, transformed);
                }

                Ok(Value::Mapping(result))
            }
            _ => {
                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                let found_type = items_result.to_type_str();

                Err(type_mismatch_error_with_path_tracker(
                    "object",
                    found_type,
                    "!$mapValues items field",
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a concat map tag
    fn resolve_concat_map(
        &self,
        tag: &ConcatMapTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let map_result = self.resolve_map_items(
            &tag.items,
            &tag.template,
            tag.var.as_deref(),
            tag.filter.as_deref(),
            "!$concatMap",
            context,
            path_tracker,
        )?;

        match map_result {
            Value::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    match item {
                        Value::Sequence(mut sub_seq) => result.append(&mut sub_seq),
                        other => result.push(other),
                    }
                }
                Ok(Value::Sequence(result))
            }
            _ => {
                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                let found_type = map_result.to_type_str();

                Err(type_mismatch_error_with_path_tracker(
                    "sequence",
                    found_type,
                    "!$concatMap map result",
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a merge map tag
    fn resolve_merge_map(
        &self,
        tag: &MergeMapTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let map_result = self.resolve_map_items(
            &tag.items,
            &tag.template,
            tag.var.as_deref(),
            None,
            "!$mergeMap",
            context,
            path_tracker,
        )?;

        match map_result {
            Value::Sequence(seq) => {
                let mut result = serde_yaml::Mapping::new();
                for item in seq {
                    match item {
                        Value::Mapping(map) => result.extend(map),
                        _ => {
                            let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                            let found_type = item.to_type_str();

                            return Err(type_mismatch_error_with_path_tracker(
                                "object",
                                found_type,
                                "!$mergeMap template item",
                                file_path,
                                path_tracker,
                            ));
                        }
                    }
                }
                Ok(Value::Mapping(result))
            }
            _ => {
                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                let found_type = map_result.to_type_str();

                Err(type_mismatch_error_with_path_tracker(
                    "sequence",
                    found_type,
                    "!$mergeMap map result",
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a map list to hash tag
    fn resolve_map_list_to_hash(
        &self,
        tag: &MapListToHashTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context, path_tracker)?;

        match items_result {
            Value::Sequence(seq) => {
                let mut result = serde_yaml::Mapping::with_capacity(seq.len());
                let var_name = tag.var.as_deref().unwrap_or("item");

                for item in seq {
                    // Apply filter if present
                    if let Some(filter) = &tag.filter {
                        let mut filter_bindings = HashMap::with_capacity(1);
                        filter_bindings.insert(var_name.to_string(), item.clone());
                        let filter_context = context.with_bindings_ref(&filter_bindings);

                        let filter_result =
                            self.resolve_ast(filter, &filter_context, path_tracker)?;
                        if !self.is_truthy(&filter_result) {
                            continue; // Skip this item
                        }
                    }

                    // Create context with current item
                    let mut item_bindings = HashMap::with_capacity(1);
                    item_bindings.insert(var_name.to_string(), item.clone());
                    let item_context = context.with_bindings_ref(&item_bindings);

                    // Apply the template transformation
                    let transformed =
                        self.resolve_ast(&tag.template, &item_context, path_tracker)?;

                    // Handle different template result formats
                    match transformed {
                        // Case 1: Template produces a 2-element array [key, value]
                        Value::Sequence(pair) if pair.len() == 2 => {
                            let key = &pair[0];
                            let value = &pair[1];
                            result.insert(key.clone(), value.clone());
                        }
                        // Case 2: Template produces the item directly (should extract key/value fields)
                        Value::Mapping(map) => {
                            // Try standard key/value fields first
                            if let (Some(key), Some(value)) = (
                                map.get(&Value::String("key".to_string())),
                                map.get(&Value::String("value".to_string())),
                            ) {
                                result.insert(key.clone(), value.clone());
                            } else {
                                // If no standard key/value, extend the result with all entries
                                result.extend(map);
                            }
                        }
                        // Case 3: Other formats - error
                        _ => {
                            let found_type = match transformed {
                                Value::Null => "null",
                                Value::Bool(_) => "boolean",
                                Value::Number(_) => "number",
                                Value::String(_) => "string",
                                Value::Sequence(ref seq) => {
                                    if seq.len() == 2 {
                                        "sequence (wrong content)"
                                    } else {
                                        "sequence (wrong length)"
                                    }
                                }
                                _ => "unknown type",
                            };

                            let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                            return Err(type_mismatch_error_with_path_tracker(
                                "2-element sequence or object",
                                found_type,
                                "!$mapListToHash template item",
                                file_path,
                                path_tracker,
                            ));
                        }
                    }
                }

                Ok(Value::Mapping(result))
            }
            _ => {
                let found_type = items_result.to_type_str();

                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                Err(type_mismatch_error_with_path_tracker(
                    "sequence",
                    found_type,
                    "!$mapListToHash items field",
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a group by tag
    fn resolve_group_by(
        &self,
        tag: &GroupByTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context, path_tracker)?;

        match items_result {
            Value::Sequence(seq) => {
                let mut groups: std::collections::HashMap<String, Vec<Value>> =
                    std::collections::HashMap::new();
                let var_name = tag.var.as_deref().unwrap_or("item");

                for item in seq {
                    // Create context with current item
                    let mut item_bindings = HashMap::with_capacity(1);
                    item_bindings.insert(var_name.to_string(), item.clone());
                    let item_context = context.with_bindings_ref(&item_bindings);

                    // Evaluate the key expression
                    let key_result = self.resolve_ast(&tag.key, &item_context, path_tracker)?;
                    let key_str = match key_result {
                        Value::String(s) => s,
                        other => serde_yaml::to_string(&other)?.trim().to_string(),
                    };

                    groups.entry(key_str).or_insert_with(Vec::new).push(item);
                }

                // Convert to mapping
                let mut result = serde_yaml::Mapping::with_capacity(groups.len());
                for (key, items) in groups {
                    result.insert(Value::String(key), Value::Sequence(items));
                }

                Ok(Value::Mapping(result))
            }
            _ => {
                let found_type = items_result.to_type_str();

                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                Err(type_mismatch_error_with_path_tracker(
                    "sequence",
                    found_type,
                    "!$groupBy items field",
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a from pairs tag
    fn resolve_from_pairs(
        &self,
        tag: &FromPairsTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.source, context, path_tracker)?;

        match items_result {
            Value::Sequence(seq) => {
                let mut result = serde_yaml::Mapping::with_capacity(seq.len());

                for item in seq {
                    match item {
                        Value::Sequence(pair) if pair.len() == 2 => {
                            let key = &pair[0];
                            let value = &pair[1];
                            result.insert(key.clone(), value.clone());
                        }
                        _ => {
                            let found_type = item.to_type_str();
                            let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                            return Err(type_mismatch_error_with_path_tracker(
                                "sequence",
                                found_type,
                                "!$fromPairs source item",
                                file_path,
                                path_tracker,
                            ));
                        }
                    }
                }

                Ok(Value::Mapping(result))
            }
            _ => {
                let found_type = items_result.to_type_str();
                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                Err(type_mismatch_error_with_path_tracker(
                    "sequence",
                    found_type,
                    "!$fromPairs source field",
                    file_path,
                    path_tracker,
                ))
            }
        }
    }

    /// Resolve a to JSON string tag
    fn resolve_to_json_string(
        &self,
        tag: &ToJsonStringTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let data_result = self.resolve_ast(&tag.data, context, path_tracker)?;

        // Convert serde_yaml::Value to serde_json::Value and then to string
        let json_value = yaml_to_json_value(&data_result)?;
        let json_string = serde_json::to_string(&json_value)
            .map_err(|e| anyhow!("Failed to serialize to JSON: {}", e))
            .with_context(|| "resolving !$toJsonString tag")?;

        Ok(Value::String(json_string))
    }

    /// Resolve a to YAML string tag
    fn resolve_to_yaml_string(
        &self,
        tag: &ToYamlStringTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let data_result = self.resolve_ast(&tag.data, context, path_tracker)?;

        let yaml_string = serde_yaml::to_string(&data_result)
            .map_err(|e| anyhow!("Failed to serialize to YAML: {}", e))
            .with_context(|| "resolving !$toYamlString tag")?;

        // Strip trailing newline to match expected format
        let trimmed_yaml = yaml_string.trim_end_matches('\n');

        Ok(Value::String(trimmed_yaml.to_string()))
    }

    /// Resolve a parse JSON tag
    fn resolve_parse_json(
        &self,
        tag: &ParseJsonTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let json_string_result = self.resolve_ast(&tag.json_string, context, path_tracker)?;

        match json_string_result {
            Value::String(json_str) => {
                let json_value: serde_json::Value = serde_json::from_str(&json_str)
                    .map_err(|e| anyhow!("Failed to parse JSON: {}", e))
                    .with_context(|| "resolving !$parseJson tag")?;

                // Convert back to serde_yaml::Value
                json_to_yaml_value(&json_value)
            }
            _ => Err(anyhow!(
                "ParseJson requires a string input, found {}",
                json_string_result.to_type_str()
            ))
            .with_context(|| "resolving !$parseJson tag"),
        }
    }

    /// Resolve a parse YAML tag
    fn resolve_parse_yaml(
        &self,
        tag: &ParseYamlTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let yaml_string_result = self.resolve_ast(&tag.yaml_string, context, path_tracker)?;

        match yaml_string_result {
            Value::String(yaml_str) => {
                let yaml_value: Value = serde_yaml::from_str(&yaml_str)
                    .map_err(|e| anyhow!("Failed to parse YAML: {}", e))
                    .with_context(|| "resolving !$parseYaml tag")?;

                Ok(yaml_value)
            }
            _ => Err(anyhow!(
                "ParseYaml requires a string input, found {}",
                yaml_string_result.to_type_str()
            ))
            .with_context(|| "resolving !$parseYaml tag"),
        }
    }

    /// Resolve an escape tag
    fn resolve_escape(
        &self,
        tag: &EscapeTag,
        _context: &TagContext,
        _path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        // Escape tag prevents preprocessing on its content
        // For now, we'll just convert the content to value without processing
        match &*tag.content {
            YamlAst::Null(_) => Ok(Value::Null),
            YamlAst::Bool(b, _) => Ok(Value::Bool(*b)),
            YamlAst::Number(n, _) => Ok(Value::Number(n.clone())),
            YamlAst::PlainString(s, _) | YamlAst::TemplatedString(s, _) => {
                Ok(Value::String(s.clone()))
            }
            YamlAst::Sequence(seq, _) => {
                let mut result = Vec::with_capacity(seq.len());
                for item in seq {
                    result.push(self.ast_to_value_without_preprocessing(item)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(pairs, _) => {
                let mut result = serde_yaml::Mapping::with_capacity(pairs.len());
                for (key, value) in pairs {
                    let key_val = self.ast_to_value_without_preprocessing(key)?;
                    let value_val = self.ast_to_value_without_preprocessing(value)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            _ => {
                // For complex types, we need to convert without processing
                self.ast_to_value_without_preprocessing(&tag.content)
            }
        }
    }

    /// Resolve a CloudFormation tag
    fn resolve_cloudformation_tag(
        &self,
        cfn_tag: &CloudFormationTag,
        context: &TagContext,
        path_tracker: &mut PathTracker,
    ) -> Result<Value> {
        let resolved_value = self.resolve_ast(cfn_tag.inner_value(), context, path_tracker)?;

        // Handle array unpacking for CloudFormation array syntax
        // If the resolved value is a single-element array, unpack it
        let final_value = match &resolved_value {
            Value::Sequence(seq) if seq.len() == 1 => seq[0].clone(),
            _ => resolved_value,
        };

        // Validate CloudFormation tag structure after processing and unpacking the value
        self.validate_cloudformation_tag(cfn_tag, &final_value, context, path_tracker)?;

        // Create a proper CloudFormation tagged value
        let tag = Tag::new(cfn_tag.tag_name());
        let tagged_value = TaggedValue {
            tag,
            value: final_value,
        };
        Ok(Value::Tagged(Box::new(tagged_value)))
    }

    /// Validate CloudFormation tag structure based on resolved values
    fn validate_cloudformation_tag(
        &self,
        cfn_tag: &CloudFormationTag,
        resolved_value: &Value,
        context: &TagContext,
        path_tracker: &PathTracker,
    ) -> Result<()> {
        use crate::yaml::parsing::ast::CloudFormationTag::*;

        match cfn_tag {
            Ref(_) => {
                // !Ref: Must resolve to a string (parameter name, resource logical ID)
                match resolved_value {
                    Value::Null => {
                        let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                        Err(cloudformation_validation_error_with_path_tracker(
                            "Ref",
                            "!Ref cannot have null value",
                            file_path,
                            path_tracker,
                        ))
                    }
                    Value::String(s) if s.is_empty() => {
                        let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                        Err(cloudformation_validation_error_with_path_tracker(
                            "Ref",
                            "!Ref cannot reference empty string",
                            file_path,
                            path_tracker,
                        ))
                    }
                    Value::String(_) => Ok(()), // Valid string reference
                    _ => {
                        let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                        let type_name = match resolved_value {
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number",
                            Value::Sequence(_) => "array",
                            Value::Mapping(_) => "object",
                            _ => "other type",
                        };
                        Err(cloudformation_validation_error_with_path_tracker(
                            "Ref",
                            &format!(
                                "!Ref expects a string (resource or parameter name), found {}",
                                type_name
                            ),
                            file_path,
                            path_tracker,
                        ))
                    }
                }
            }

            Sub(_) => {
                // !Sub: Can be a string or a 2-element array [string, variables]
                match resolved_value {
                    Value::Null => Err(anyhow!("!Sub cannot have null value"))
                        .with_context(|| format!("validating !Sub at path /{}", path_tracker.segments().join("/"))),
                    Value::String(_) => Ok(()), // Valid string for substitution
                    Value::Sequence(seq) if seq.len() == 2 => {
                        // First element should be string, second should be mapping
                        match (&seq[0], &seq[1]) {
                            (Value::String(_), Value::Mapping(_)) => Ok(()),
                            (Value::String(_), _) => Err(anyhow!("!Sub array form expects [string, object], found [string, {}]", 
                                match &seq[1] {
                                    Value::Null => "null",
                                    Value::Bool(_) => "boolean",
                                    Value::Number(_) => "number",
                                    Value::String(_) => "string",
                                    Value::Sequence(_) => "array",
                                    _ => "other"
                                }))
                                .with_context(|| format!("validating !Sub at path /{}", path_tracker.segments().join("/"))),
                            (_, _) => Err(anyhow!("!Sub array form expects [string, object], found [{}, {}]",
                                match &seq[0] {
                                    Value::String(_) => "string",
                                    Value::Null => "null",
                                    Value::Bool(_) => "boolean",
                                    Value::Number(_) => "number",
                                    _ => "other"
                                },
                                match &seq[1] {
                                    Value::Mapping(_) => "object",
                                    Value::Null => "null",
                                    Value::String(_) => "string",
                                    _ => "other"
                                }))
                                .with_context(|| format!("validating !Sub at path /{}", path_tracker.segments().join("/"))),
                        }
                    },
                    Value::Sequence(seq) => Err(anyhow!("!Sub with array expects exactly 2 elements [string, variables], found {} elements", seq.len()))
                        .with_context(|| format!("validating !Sub at path /{}", path_tracker.segments().join("/"))),
                    _ => Err(anyhow!("!Sub expects a string or 2-element array, found {}", 
                        match resolved_value {
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number", 
                            Value::Mapping(_) => "object",
                            _ => "other type"
                        }))
                        .with_context(|| format!("validating !Sub at path /{}", path_tracker.segments().join("/"))),
                }
            }

            GetAtt(_) => {
                // !GetAtt: Must be string "Resource.Attribute" or 2-element array ["Resource", "Attribute"]
                match resolved_value {
                    Value::Null => Err(anyhow!("!GetAtt cannot have null value"))
                        .with_context(|| format!("validating !GetAtt at path /{}", path_tracker.segments().join("/"))),
                    Value::String(s) if s.contains('.') => Ok(()), // Valid dot notation
                    Value::String(_) => Err(anyhow!("!GetAtt string format requires dot notation: 'ResourceName.AttributeName'"))
                        .with_context(|| format!("validating !GetAtt at path /{}", path_tracker.segments().join("/"))),
                    Value::Sequence(seq) if seq.len() == 2 => {
                        // Both elements should be strings
                        match (&seq[0], &seq[1]) {
                            (Value::String(_), Value::String(_)) => Ok(()),
                            _ => Err(anyhow!("!GetAtt array form expects [string, string], found [{}, {}]",
                                match &seq[0] {
                                    Value::String(_) => "string",
                                    Value::Null => "null",
                                    _ => "other"
                                },
                                match &seq[1] {
                                    Value::String(_) => "string", 
                                    Value::Null => "null",
                                    _ => "other"
                                }))
                                .with_context(|| format!("validating !GetAtt at path /{}", path_tracker.segments().join("/"))),
                        }
                    },
                    Value::Sequence(seq) => Err(anyhow!("!GetAtt expects exactly 2 elements [resource, attribute], found {} elements", seq.len()))
                        .with_context(|| format!("validating !GetAtt at path /{}", path_tracker.segments().join("/"))),
                    _ => Err(anyhow!("!GetAtt expects a string or 2-element array, found {}", 
                        match resolved_value {
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number",
                            Value::Mapping(_) => "object",
                            _ => "other type"
                        }))
                        .with_context(|| format!("validating !GetAtt at path /{}", path_tracker.segments().join("/"))),
                }
            }

            Join(_) => {
                // !Join: Must be 2-element array [delimiter, values]
                match resolved_value {
                    Value::Null => {
                        let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                        Err(cloudformation_validation_error_with_path_tracker(
                            "Join",
                            "!Join cannot have null value",
                            file_path,
                            path_tracker,
                        ))
                    }
                    Value::Sequence(seq) if seq.len() == 2 => {
                        // First element should be string, second should be array
                        match (&seq[0], &seq[1]) {
                            (Value::String(_), Value::Sequence(_)) => Ok(()),
                            (Value::String(_), _) => {
                                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                                let type_name = match &seq[1] {
                                    Value::Null => "null",
                                    Value::String(_) => "string",
                                    Value::Bool(_) => "boolean",
                                    Value::Number(_) => "number",
                                    Value::Mapping(_) => "object",
                                    _ => "other",
                                };
                                Err(cloudformation_validation_error_with_path_tracker(
                                    "Join",
                                    &format!(
                                        "!Join expects [delimiter, array], found [string, {}]",
                                        type_name
                                    ),
                                    file_path,
                                    path_tracker,
                                ))
                            }
                            (_, _) => {
                                let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                                let first_type = match &seq[0] {
                                    Value::String(_) => "string",
                                    Value::Null => "null",
                                    _ => "other",
                                };
                                let second_type = match &seq[1] {
                                    Value::Sequence(_) => "array",
                                    _ => "other",
                                };
                                Err(cloudformation_validation_error_with_path_tracker(
                                    "Join",
                                    &format!(
                                        "!Join expects [string, array], found [{}, {}]",
                                        first_type, second_type
                                    ),
                                    file_path,
                                    path_tracker,
                                ))
                            }
                        }
                    }
                    Value::Sequence(seq) => {
                        let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                        Err(cloudformation_validation_error_with_path_tracker(
                            "Join",
                            &format!(
                                "!Join expects exactly 2 elements [delimiter, array], found {} elements",
                                seq.len()
                            ),
                            file_path,
                            path_tracker,
                        ))
                    }
                    _ => {
                        let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                        let type_name = match resolved_value {
                            Value::String(_) => "string",
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number",
                            Value::Mapping(_) => "object",
                            _ => "other type",
                        };
                        Err(cloudformation_validation_error_with_path_tracker(
                            "Join",
                            &format!("!Join expects a 2-element array, found {}", type_name),
                            file_path,
                            path_tracker,
                        ))
                    }
                }
            }

            Select(_) => {
                // !Select: Must be 2-element array [index, list]
                match resolved_value {
                    Value::Null => {
                        Err(anyhow!("!Select cannot have null value")).with_context(|| {
                            format!(
                                "validating !Select at path /{}",
                                path_tracker.segments().join("/")
                            )
                        })
                    }
                    Value::Sequence(seq) if seq.len() == 2 => {
                        // First element should be number, second should be array
                        match (&seq[0], &seq[1]) {
                            (Value::Number(_), Value::Sequence(_)) => Ok(()),
                            (Value::Number(_), _) => Err(anyhow!(
                                "!Select expects [index, array], found [number, {}]",
                                match &seq[1] {
                                    Value::Null => "null",
                                    Value::String(_) => "string",
                                    Value::Bool(_) => "boolean",
                                    Value::Number(_) => "number",
                                    Value::Mapping(_) => "object",
                                    _ => "other",
                                }
                            ))
                            .with_context(|| {
                                format!(
                                    "validating !Select at path /{}",
                                    path_tracker.segments().join("/")
                                )
                            }),
                            (_, _) => Err(anyhow!(
                                "!Select expects [number, array], found [{}, {}]",
                                match &seq[0] {
                                    Value::Number(_) => "number",
                                    Value::String(_) => "string",
                                    _ => "other",
                                },
                                match &seq[1] {
                                    Value::Sequence(_) => "array",
                                    _ => "other",
                                }
                            ))
                            .with_context(|| {
                                format!(
                                    "validating !Select at path /{}",
                                    path_tracker.segments().join("/")
                                )
                            }),
                        }
                    }
                    Value::Sequence(seq) => Err(anyhow!(
                        "!Select expects exactly 2 elements [index, array], found {} elements",
                        seq.len()
                    ))
                    .with_context(|| {
                        format!(
                            "validating !Select at path /{}",
                            path_tracker.segments().join("/")
                        )
                    }),
                    _ => Err(anyhow!(
                        "!Select expects a 2-element array, found {}",
                        match resolved_value {
                            Value::String(_) => "string",
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number",
                            Value::Mapping(_) => "object",
                            _ => "other type",
                        }
                    ))
                    .with_context(|| {
                        format!(
                            "validating !Select at path /{}",
                            path_tracker.segments().join("/")
                        )
                    }),
                }
            }

            Split(_) => {
                // !Split: Must be 2-element array [delimiter, string]
                match resolved_value {
                    Value::Null => {
                        Err(anyhow!("!Split cannot have null value")).with_context(|| {
                            format!(
                                "validating !Split at path /{}",
                                path_tracker.segments().join("/")
                            )
                        })
                    }
                    Value::Sequence(seq) if seq.len() == 2 => {
                        // Both elements should be strings
                        match (&seq[0], &seq[1]) {
                            (Value::String(_), Value::String(_)) => Ok(()),
                            _ => Err(anyhow!(
                                "!Split expects [string, string], found [{}, {}]",
                                match &seq[0] {
                                    Value::String(_) => "string",
                                    _ => "other",
                                },
                                match &seq[1] {
                                    Value::String(_) => "string",
                                    _ => "other",
                                }
                            ))
                            .with_context(|| {
                                format!(
                                    "validating !Split at path /{}",
                                    path_tracker.segments().join("/")
                                )
                            }),
                        }
                    }
                    Value::Sequence(seq) => Err(anyhow!(
                        "!Split expects exactly 2 elements [delimiter, string], found {} elements",
                        seq.len()
                    ))
                    .with_context(|| {
                        format!(
                            "validating !Split at path /{}",
                            path_tracker.segments().join("/")
                        )
                    }),
                    _ => Err(anyhow!(
                        "!Split expects a 2-element array, found {}",
                        match resolved_value {
                            Value::String(_) => "string",
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number",
                            Value::Mapping(_) => "object",
                            _ => "other type",
                        }
                    ))
                    .with_context(|| {
                        format!(
                            "validating !Split at path /{}",
                            path_tracker.segments().join("/")
                        )
                    }),
                }
            }

            FindInMap(_) => {
                // !FindInMap: Must be 3-element array [map_name, key1, key2]
                match resolved_value {
                    Value::Null => Err(anyhow!("!FindInMap cannot have null value"))
                        .with_context(|| format!("validating !FindInMap at path /{}", path_tracker.segments().join("/"))),
                    Value::Sequence(seq) if seq.len() == 3 => Ok(()), // Valid 3-element array
                    Value::Sequence(seq) => Err(anyhow!("!FindInMap expects exactly 3 elements [map_name, key1, key2], found {} elements", seq.len()))
                        .with_context(|| format!("validating !FindInMap at path /{}", path_tracker.segments().join("/"))),
                    _ => Err(anyhow!("!FindInMap expects a 3-element array, found {}", 
                        match resolved_value {
                            Value::String(_) => "string",
                            Value::Bool(_) => "boolean",
                            Value::Number(_) => "number",
                            Value::Mapping(_) => "object",
                            _ => "other type"
                        }))
                        .with_context(|| format!("validating !FindInMap at path /{}", path_tracker.segments().join("/"))),
                }
            }

            // For remaining tags, just check for null values
            _ => match resolved_value {
                Value::Null => {
                    let file_path = context.input_uri.as_deref().unwrap_or("unknown");
                    Err(cloudformation_validation_error_with_path_tracker(
                        cfn_tag.tag_name(),
                        &format!("!{} cannot have null value", cfn_tag.tag_name()),
                        file_path,
                        path_tracker,
                    ))
                }
                _ => Ok(()),
            },
        }
    }
}

/// Convenience function to resolve AST with automatic path tracker creation
pub fn resolve_ast(ast: &YamlAst, context: &TagContext) -> Result<Value> {
    let resolver = Resolver;
    let mut path_tracker = PathTracker::new();
    resolver.resolve_ast(ast, context, &mut path_tracker)
}

/// Convert serde_json::Value to serde_yaml::Value
fn json_to_yaml_value(json_value: &serde_json::Value) -> Result<Value> {
    match json_value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Number(serde_yaml::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                Ok(Value::Number(serde_yaml::Number::from(u)))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(serde_yaml::Number::from(f)))
            } else {
                Ok(Value::Null)
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut yaml_seq = Vec::with_capacity(arr.len());
            for item in arr {
                yaml_seq.push(json_to_yaml_value(item)?);
            }
            Ok(Value::Sequence(yaml_seq))
        }
        serde_json::Value::Object(obj) => {
            let mut yaml_map = serde_yaml::Mapping::with_capacity(obj.len());
            for (key, value) in obj {
                yaml_map.insert(Value::String(key.clone()), json_to_yaml_value(value)?);
            }
            Ok(Value::Mapping(yaml_map))
        }
    }
}

/// Extract variable name from a handlebars strict-mode error message.
/// The handlebars crate formats these as: `Variable "name" not found in strict mode.`
fn parse_variable_name_from_handlebars_error(error_msg: &str) -> Option<&str> {
    let marker = "Variable \"";
    let start = error_msg.find(marker)? + marker.len();
    let end = start + error_msg[start..].find('"')?;
    Some(&error_msg[start..end])
}

/// Try to find which line of a source file contains a handlebars reference to `var_name`.
/// Returns "file_path:line" if found, otherwise just "file_path".
/// Note: blocking read in a sync function called from async context. Acceptable because
/// this is an error-only path reading a local file already loaded by the resolver.
fn find_template_variable_location(file_path: &str, var_name: &str) -> String {
    let needle = format!("{{{{{}}}}}", var_name);
    if let Ok(content) = std::fs::read_to_string(file_path) {
        if let Some(line_number) = content.lines().position(|line| line.contains(&needle)) {
            return format!("{}:{}", file_path, line_number + 1);
        }
    }
    file_path.to_string()
}

/// Convert serde_yaml::Value to serde_json::Value for handlebars processing
pub fn yaml_to_json_value(yaml_value: &Value) -> Result<serde_json::Value> {
    match yaml_value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(u)))
            } else if let Some(f) = n.as_f64() {
                Ok(serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Sequence(seq) => {
            let mut json_seq = Vec::with_capacity(seq.len());
            for item in seq {
                json_seq.push(yaml_to_json_value(item)?);
            }
            Ok(serde_json::Value::Array(json_seq))
        }
        Value::Mapping(map) => {
            let mut json_map = serde_json::Map::with_capacity(map.len());
            for (key, value) in map {
                let key_str = match key {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    other => format!("{:?}", other),
                };
                json_map.insert(key_str, yaml_to_json_value(value)?);
            }
            Ok(serde_json::Value::Object(json_map))
        }
        Value::Tagged(tagged) => {
            // Handle tagged values by converting the inner value
            yaml_to_json_value(&tagged.value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    // Helper function to create dummy SrcMeta for tests
    fn dummy_src_meta() -> SrcMeta {
        SrcMeta {
            input_uri: Url::parse("file:///test.yaml").unwrap(),
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        }
    }

    #[test]
    fn test_path_tracker_basic_operations() {
        let mut tracker = PathTracker::new();

        tracker.push("config");
        tracker.push("database");
        assert_eq!(tracker.current_path(), "config.database");

        tracker.pop();
        assert_eq!(tracker.current_path(), "config");

        tracker.clear();
        assert_eq!(tracker.current_path(), "");
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_path_tracker_push_pop() {
        let mut tracker = PathTracker::new();
        tracker.push("root");
        tracker.push("temp");

        assert_eq!(tracker.current_path(), "root.temp");

        tracker.pop();
        assert_eq!(tracker.current_path(), "root");
    }

    #[test]
    fn test_path_tracker_array_index() {
        let mut tracker = PathTracker::new();
        tracker.push("array");
        tracker.push("[5]");

        assert_eq!(tracker.current_path(), "array.[5]");

        tracker.pop();
        assert_eq!(tracker.current_path(), "array");
    }

    #[test]
    fn test_split_args_simple_mapping() {
        let resolver = Resolver;
        let context = TagContext::new();
        let mut path_tracker = PathTracker::new();

        // Create a simple mapping AST
        let key = YamlAst::PlainString("test_key".to_string(), dummy_src_meta());
        let value = YamlAst::PlainString("test_value".to_string(), dummy_src_meta());
        let mapping = YamlAst::Mapping(vec![(key, value)], dummy_src_meta());

        let result = resolver
            .resolve_ast(&mapping, &context, &mut path_tracker)
            .unwrap();

        match result {
            Value::Mapping(map) => {
                assert_eq!(map.len(), 1);
                assert_eq!(
                    map.get(&Value::String("test_key".to_string())),
                    Some(&Value::String("test_value".to_string()))
                );
            }
            _ => panic!("Expected mapping result"),
        }
    }

    #[test]
    fn test_split_args_nested_structure() {
        let resolver = Resolver;
        let context = TagContext::new();
        let mut path_tracker = PathTracker::new();

        // Create nested structure: {outer: {inner: "value"}}
        let inner_key = YamlAst::PlainString("inner".to_string(), dummy_src_meta());
        let inner_value = YamlAst::PlainString("value".to_string(), dummy_src_meta());
        let inner_mapping = YamlAst::Mapping(vec![(inner_key, inner_value)], dummy_src_meta());

        let outer_key = YamlAst::PlainString("outer".to_string(), dummy_src_meta());
        let outer_mapping = YamlAst::Mapping(vec![(outer_key, inner_mapping)], dummy_src_meta());

        let result = resolver
            .resolve_ast(&outer_mapping, &context, &mut path_tracker)
            .unwrap();

        // Verify the nested structure was resolved correctly
        match result {
            Value::Mapping(outer_map) => {
                assert_eq!(outer_map.len(), 1);
                if let Some(Value::Mapping(inner_map)) =
                    outer_map.get(&Value::String("outer".to_string()))
                {
                    assert_eq!(
                        inner_map.get(&Value::String("inner".to_string())),
                        Some(&Value::String("value".to_string()))
                    );
                } else {
                    panic!("Expected nested mapping");
                }
            }
            _ => panic!("Expected mapping result"),
        }
    }

    #[test]
    fn test_path_tracker_capacity() {
        let tracker = PathTracker::with_capacity(10);
        assert_eq!(tracker.len(), 0);
        // SmallVec should have pre-allocated capacity
    }
}
