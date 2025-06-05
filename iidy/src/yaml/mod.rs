//! YAML preprocessing module for iidy
//! 
//! This module implements the custom YAML preprocessing language that allows
//! advanced template composition, data imports, and transformations.

pub mod ast;
pub mod parser;
pub mod tags;
pub mod imports;
pub mod handlebars;

pub use ast::*;
pub use parser::parse_yaml_with_custom_tags;
pub use tags::TagContext;

use anyhow::Result;
use serde_yaml::Value;

/// Main entry point for YAML preprocessing
/// 
/// Takes raw YAML text and processes all custom tags and preprocessing directives
/// to produce a final YAML document ready for standard deserialization.
pub fn preprocess_yaml(input: &str) -> Result<Value> {
    let mut parser = YamlPreprocessor::new();
    parser.process(input)
}

/// YAML preprocessor that handles custom tags and preprocessing directives
pub struct YamlPreprocessor {
    // Future: Add context for imports, variables, etc.
}

impl YamlPreprocessor {
    pub fn new() -> Self {
        Self {}
    }

    pub fn process(&mut self, input: &str) -> Result<Value> {
        // Parse YAML with custom tag support
        let ast = parser::parse_yaml_with_custom_tags(input)?;
        
        // Process the AST and resolve all custom tags
        self.resolve_ast(ast)
    }

    pub fn resolve_ast(&mut self, ast: YamlAst) -> Result<Value> {
        self.resolve_ast_with_context(ast, &TagContext::new())
    }

    pub fn resolve_ast_with_context(&mut self, ast: YamlAst, context: &TagContext) -> Result<Value> {
        match ast {
            YamlAst::Null => Ok(Value::Null),
            YamlAst::Bool(b) => Ok(Value::Bool(b)),
            YamlAst::Number(n) => Ok(Value::Number(serde_yaml::Number::from(n))),
            YamlAst::String(s) => {
                // Process handlebars templates in strings
                self.process_string_with_handlebars(s, context)
            },
            YamlAst::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    result.push(self.resolve_ast_with_context(item, context)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                for (key, value) in map {
                    let key_val = self.resolve_ast_with_context(key, context)?;
                    let value_val = self.resolve_ast_with_context(value, context)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(tag) => {
                self.resolve_preprocessing_tag_with_context(tag, context)
            },
            YamlAst::UnknownYamlTag(_) => todo!()
        }
    }

    fn resolve_preprocessing_tag(&mut self, tag: PreprocessingTag) -> Result<Value> {
        self.resolve_preprocessing_tag_with_context(tag, &TagContext::new())
    }

    fn process_string_with_handlebars(&self, s: String, context: &TagContext) -> Result<Value> {
        use crate::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;
        
        // Convert TagContext variables from serde_yaml::Value to serde_json::Value
        let mut env_values: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, yaml_value) in &context.variables {
            let json_value = yaml_value_to_json_value(yaml_value)?;
            env_values.insert(key.clone(), json_value);
        }
        
        // Apply handlebars interpolation to the string
        match interpolate_handlebars_string(&s, &env_values, "yaml-string") {
            Ok(processed_string) => Ok(Value::String(processed_string)),
            Err(e) => Err(anyhow::anyhow!("Handlebars processing failed: {}", e)),
        }
    }

    fn resolve_preprocessing_tag_with_context(&mut self, tag: PreprocessingTag, context: &TagContext) -> Result<Value> {
        use crate::yaml::tags::*;
        
        match tag {
            PreprocessingTag::Include(include_tag) => {
                resolve_include_tag(&include_tag, context)
            }
            PreprocessingTag::If(if_tag) => {
                resolve_if_tag(&if_tag, context, self)
            }
            PreprocessingTag::Map(map_tag) => {
                resolve_map_tag(&map_tag, context, self)
            }
            PreprocessingTag::Merge(merge_tag) => {
                resolve_merge_tag(&merge_tag, context, self)
            }
            PreprocessingTag::Concat(concat_tag) => {
                resolve_concat_tag(&concat_tag, context, self)
            }
            PreprocessingTag::Let(let_tag) => {
                resolve_let_tag(&let_tag, context, self)
            }
            PreprocessingTag::Eq(eq_tag) => {
                resolve_eq_tag(&eq_tag, context, self)
            }
            PreprocessingTag::Not(not_tag) => {
                resolve_not_tag(&not_tag, context, self)
            }
            PreprocessingTag::Split(split_tag) => {
                resolve_split_tag(&split_tag, context, self)
            }
            PreprocessingTag::Join(join_tag) => {
                resolve_join_tag(&join_tag, context, self)
            }
        }
    }
}

impl Default for YamlPreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl tags::AstResolver for YamlPreprocessor {
    fn resolve_ast(&self, ast: &YamlAst, context: &tags::TagContext) -> Result<Value> {
        // Need to clone to work around mutable borrow
        let mut cloned_self = YamlPreprocessor::new();
        cloned_self.resolve_ast_with_context(ast.clone(), context)
    }
}

/// Convert serde_yaml::Value to serde_json::Value for handlebars processing
fn yaml_value_to_json_value(yaml_value: &Value) -> Result<serde_json::Value> {
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
            let mut json_seq = Vec::new();
            for item in seq {
                json_seq.push(yaml_value_to_json_value(item)?);
            }
            Ok(serde_json::Value::Array(json_seq))
        }
        Value::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                let key_str = match k {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.as_f64().unwrap_or(0.0).to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", k), // fallback for other types
                };
                json_map.insert(key_str, yaml_value_to_json_value(v)?);
            }
            Ok(serde_json::Value::Object(json_map))
        }
        Value::Tagged(_) => Err(anyhow::anyhow!("Tagged values not supported in handlebars conversion")),
    }
}
