//! Tests for the TagResolver trait system
//!
//! Verifies that different tag resolver implementations work correctly

use anyhow::Result;
use iidy::yaml::tags::{TagResolver, StandardTagResolver, DebugTagResolver, TracingTagResolver, TagContext};
use iidy::yaml::ast::{IncludeTag, IfTag, YamlAst};
use serde_yaml::Value;

struct MockAstResolver;

impl iidy::yaml::tags::AstResolver for MockAstResolver {
    fn resolve_ast(&self, ast: &YamlAst, _context: &TagContext) -> Result<Value> {
        match ast {
            YamlAst::Bool(b) => Ok(Value::Bool(*b)),
            YamlAst::String(s) => Ok(Value::String(s.clone())),
            YamlAst::Number(n) => Ok(Value::Number(n.clone())),
            _ => Ok(Value::String("mock-resolved".to_string())),
        }
    }
}

#[test]
fn test_standard_tag_resolver() -> Result<()> {
    let resolver = StandardTagResolver;
    let context = TagContext::new()
        .with_variable("test_var", Value::String("test_value".to_string()));
    
    // Test include tag
    let include_tag = IncludeTag {
        path: "test_var".to_string(),
        query: None,
    };
    
    let result = resolver.resolve_include(&include_tag, &context)?;
    assert_eq!(result, Value::String("test_value".to_string()));
    
    Ok(())
}

#[test]
fn test_debug_tag_resolver() -> Result<()> {
    let resolver = DebugTagResolver::new();
    let context = TagContext::new();
    let ast_resolver = MockAstResolver;
    
    // Test if tag - this will print debug output
    let if_tag = IfTag {
        test: Box::new(YamlAst::Bool(true)),
        then_value: Box::new(YamlAst::String("then_result".to_string())),
        else_value: Some(Box::new(YamlAst::String("else_result".to_string()))),
    };
    
    let result = resolver.resolve_if(&if_tag, &context, &ast_resolver)?;
    assert_eq!(result, Value::String("then_result".to_string()));
    
    Ok(())
}

#[test]
fn test_tracing_tag_resolver() -> Result<()> {
    let resolver = TracingTagResolver::new();
    let context = TagContext::new();
    let ast_resolver = MockAstResolver;
    
    // Test if tag - this will print timing output
    let if_tag = IfTag {
        test: Box::new(YamlAst::Bool(false)),
        then_value: Box::new(YamlAst::String("then_result".to_string())),
        else_value: Some(Box::new(YamlAst::String("else_result".to_string()))),
    };
    
    let result = resolver.resolve_if(&if_tag, &context, &ast_resolver)?;
    assert_eq!(result, Value::String("else_result".to_string()));
    
    Ok(())
}

#[test]
fn test_resolver_trait_consistency() -> Result<()> {
    // Test that all resolvers produce the same results for the same input
    let standard = StandardTagResolver;
    let debug = DebugTagResolver::new();
    let tracing = TracingTagResolver::new();
    
    let context = TagContext::new()
        .with_variable("config", Value::String("config_value".to_string()));
    let ast_resolver = MockAstResolver;
    
    let if_tag = IfTag {
        test: Box::new(YamlAst::Bool(true)),
        then_value: Box::new(YamlAst::String("success".to_string())),
        else_value: None,
    };
    
    let standard_result = standard.resolve_if(&if_tag, &context, &ast_resolver)?;
    let debug_result = debug.resolve_if(&if_tag, &context, &ast_resolver)?;
    let tracing_result = tracing.resolve_if(&if_tag, &context, &ast_resolver)?;
    
    assert_eq!(standard_result, debug_result);
    assert_eq!(standard_result, tracing_result);
    assert_eq!(standard_result, Value::String("success".to_string()));
    
    Ok(())
}

#[test]
fn test_resolver_extensibility() -> Result<()> {
    // Create a custom resolver that modifies behavior
    struct CustomTagResolver {
        inner: StandardTagResolver,
        prefix: String,
    }
    
    impl TagResolver for CustomTagResolver {
        fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
            let result = self.inner.resolve_include(tag, context)?;
            match result {
                Value::String(s) => Ok(Value::String(format!("{}{}", self.prefix, s))),
                other => Ok(other),
            }
        }
        
        // Delegate all other methods to the inner resolver
        fn resolve_if(&self, tag: &IfTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_if(tag, context, ast_resolver)
        }
        
        fn resolve_map(&self, tag: &iidy::yaml::ast::MapTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_map(tag, context, ast_resolver)
        }
        
        fn resolve_merge(&self, tag: &iidy::yaml::ast::MergeTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_merge(tag, context, ast_resolver)
        }
        
        fn resolve_concat(&self, tag: &iidy::yaml::ast::ConcatTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_concat(tag, context, ast_resolver)
        }
        
        fn resolve_let(&self, tag: &iidy::yaml::ast::LetTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_let(tag, context, ast_resolver)
        }
        
        fn resolve_eq(&self, tag: &iidy::yaml::ast::EqTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_eq(tag, context, ast_resolver)
        }
        
        fn resolve_not(&self, tag: &iidy::yaml::ast::NotTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_not(tag, context, ast_resolver)
        }
        
        fn resolve_split(&self, tag: &iidy::yaml::ast::SplitTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_split(tag, context, ast_resolver)
        }
        
        fn resolve_join(&self, tag: &iidy::yaml::ast::JoinTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_join(tag, context, ast_resolver)
        }
        
        fn resolve_concat_map(&self, tag: &iidy::yaml::ast::ConcatMapTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_concat_map(tag, context, ast_resolver)
        }
        
        fn resolve_merge_map(&self, tag: &iidy::yaml::ast::MergeMapTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_merge_map(tag, context, ast_resolver)
        }
        
        fn resolve_map_list_to_hash(&self, tag: &iidy::yaml::ast::MapListToHashTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_map_list_to_hash(tag, context, ast_resolver)
        }
        
        fn resolve_map_values(&self, tag: &iidy::yaml::ast::MapValuesTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_map_values(tag, context, ast_resolver)
        }
        
        fn resolve_group_by(&self, tag: &iidy::yaml::ast::GroupByTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_group_by(tag, context, ast_resolver)
        }
        
        fn resolve_from_pairs(&self, tag: &iidy::yaml::ast::FromPairsTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_from_pairs(tag, context, ast_resolver)
        }
        
        fn resolve_to_yaml_string(&self, tag: &iidy::yaml::ast::ToYamlStringTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_to_yaml_string(tag, context, ast_resolver)
        }
        
        fn resolve_parse_yaml(&self, tag: &iidy::yaml::ast::ParseYamlTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_parse_yaml(tag, context, ast_resolver)
        }
        
        fn resolve_to_json_string(&self, tag: &iidy::yaml::ast::ToJsonStringTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_to_json_string(tag, context, ast_resolver)
        }
        
        fn resolve_parse_json(&self, tag: &iidy::yaml::ast::ParseJsonTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_parse_json(tag, context, ast_resolver)
        }
        
        fn resolve_escape(&self, tag: &iidy::yaml::ast::EscapeTag, context: &TagContext, ast_resolver: &dyn iidy::yaml::tags::AstResolver) -> Result<Value> {
            self.inner.resolve_escape(tag, context, ast_resolver)
        }
    }
    
    let custom_resolver = CustomTagResolver {
        inner: StandardTagResolver,
        prefix: "CUSTOM: ".to_string(),
    };
    
    let context = TagContext::new()
        .with_variable("test", Value::String("value".to_string()));
    
    let include_tag = IncludeTag {
        path: "test".to_string(),
        query: None,
    };
    
    let result = custom_resolver.resolve_include(&include_tag, &context)?;
    assert_eq!(result, Value::String("CUSTOM: value".to_string()));
    
    Ok(())
}