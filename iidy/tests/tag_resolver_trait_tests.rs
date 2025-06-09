//! Tests for the TagResolver trait system
//!
//! Verifies that different tag resolver implementations work correctly

use anyhow::Result;
use iidy::yaml::resolution::{TagResolver, StandardTagResolver, TagContext};
use iidy::yaml::resolution::resolver::{DebugTagResolver, TracingTagResolver};
use iidy::yaml::parsing::ast::{IncludeTag, IfTag, YamlAst, MapTag, MergeTag, ConcatTag, LetTag, EqTag, NotTag, SplitTag, JoinTag, ConcatMapTag, MergeMapTag, MapListToHashTag, MapValuesTag, GroupByTag, FromPairsTag, ToYamlStringTag, ParseYamlTag, ToJsonStringTag, ParseJsonTag, EscapeTag};
use serde_yaml::Value;


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
    
    // Test if tag - this will print debug output
    let if_tag = IfTag {
        test: Box::new(YamlAst::Bool(true)),
        then_value: Box::new(YamlAst::PlainString("then_result".to_string())),
        else_value: Some(Box::new(YamlAst::PlainString("else_result".to_string()))),
    };
    
    let result = resolver.resolve_if(&if_tag, &context)?;
    assert_eq!(result, Value::String("then_result".to_string()));
    
    Ok(())
}

#[test]
fn test_tracing_tag_resolver() -> Result<()> {
    let resolver = TracingTagResolver::new();
    let context = TagContext::new();
    
    // Test if tag - this will print timing output
    let if_tag = IfTag {
        test: Box::new(YamlAst::Bool(false)),
        then_value: Box::new(YamlAst::PlainString("then_result".to_string())),
        else_value: Some(Box::new(YamlAst::PlainString("else_result".to_string()))),
    };
    
    let result = resolver.resolve_if(&if_tag, &context)?;
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
    
    let if_tag = IfTag {
        test: Box::new(YamlAst::Bool(true)),
        then_value: Box::new(YamlAst::PlainString("success".to_string())),
        else_value: None,
    };
    
    let standard_result = standard.resolve_if(&if_tag, &context)?;
    let debug_result = debug.resolve_if(&if_tag, &context)?;
    let tracing_result = tracing.resolve_if(&if_tag, &context)?;
    
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
        fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value> {
            self.inner.resolve_ast(ast, context)
        }
        
        fn yaml_value_to_json_value(&self, yaml_value: &serde_yaml::Value) -> Result<serde_json::Value> {
            self.inner.yaml_value_to_json_value(yaml_value)
        }
        
        fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
            let result = self.inner.resolve_include(tag, context)?;
            match result {
                Value::String(s) => Ok(Value::String(format!("{}{}", self.prefix, s))),
                other => Ok(other),
            }
        }
        
        // Delegate all other methods to the inner resolver
        fn resolve_if(&self, tag: &IfTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_if(tag, context)
        }
        
        fn resolve_map(&self, tag: &MapTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_map(tag, context)
        }
        
        fn resolve_merge(&self, tag: &MergeTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_merge(tag, context)
        }
        
        fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_concat(tag, context)
        }
        
        fn resolve_let(&self, tag: &LetTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_let(tag, context)
        }
        
        fn resolve_eq(&self, tag: &EqTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_eq(tag, context)
        }
        
        fn resolve_not(&self, tag: &NotTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_not(tag, context)
        }
        
        fn resolve_split(&self, tag: &SplitTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_split(tag, context)
        }
        
        fn resolve_join(&self, tag: &JoinTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_join(tag, context)
        }
        
        fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_concat_map(tag, context)
        }
        
        fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_merge_map(tag, context)
        }
        
        fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_map_list_to_hash(tag, context)
        }
        
        fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_map_values(tag, context)
        }
        
        fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_group_by(tag, context)
        }
        
        fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_from_pairs(tag, context)
        }
        
        fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_to_yaml_string(tag, context)
        }
        
        fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_parse_yaml(tag, context)
        }
        
        fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_to_json_string(tag, context)
        }
        
        fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_parse_json(tag, context)
        }
        
        fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext) -> Result<Value> {
            self.inner.resolve_escape(tag, context)
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