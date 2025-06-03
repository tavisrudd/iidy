//! YAML preprocessing module for iidy
//! 
//! This module implements the custom YAML preprocessing language that allows
//! advanced template composition, data imports, and transformations.

pub mod ast;
pub mod parser;
pub mod tags;

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

    fn resolve_ast_with_context(&mut self, ast: YamlAst, context: &TagContext) -> Result<Value> {
        match ast {
            YamlAst::Scalar(s) => Ok(Value::String(s)),
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
