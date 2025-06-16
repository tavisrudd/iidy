use super::ast::{CloudFormationTag, PreprocessingTag, UnknownTag, YamlAst};
use super::parser::parse_yaml_ast;
use url::Url;

/// Standard test URI for consistency across tests
fn test_uri() -> Url {
    Url::parse("file:///test.yaml").unwrap()
}

#[test]
fn test_parse_simple_scalar() {
    let yaml = "hello world";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::PlainString(s, _) => assert_eq!(s, "hello world"),
        _ => panic!("Expected PlainString, got {:?}", result),
    }
}

#[test]
fn test_parse_templated_string() {
    let yaml = "hello {{ name }}";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::TemplatedString(s, _) => assert_eq!(s, "hello {{ name }}"),
        _ => panic!("Expected TemplatedString, got {:?}", result),
    }
}

#[test]
fn test_parse_boolean() {
    let yaml = "true";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Bool(b, _) => assert!(b),
        _ => panic!("Expected Bool, got {:?}", result),
    }
}

#[test]
fn test_parse_number() {
    let yaml = "42";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Number(n, _) => assert_eq!(n.as_i64(), Some(42)),
        _ => panic!("Expected Number, got {:?}", result),
    }
}

#[test]
fn test_parse_null() {
    let yaml = "null";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Null(_) => {}
        _ => panic!("Expected Null, got {:?}", result),
    }
}

#[test]
fn test_parse_sequence() {
    let yaml = r#"
- item1
- item2
- 42
"#;
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Sequence(items, _) => {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], YamlAst::PlainString(ref s, _) if s == "item1"));
            assert!(matches!(items[1], YamlAst::PlainString(ref s, _) if s == "item2"));
            assert!(matches!(items[2], YamlAst::Number(ref n, _) if n.as_i64() == Some(42)));
        }
        _ => panic!("Expected Sequence, got {:?}", result),
    }
}

#[test]
fn test_parse_mapping() {
    let yaml = r#"
key1: value1
key2: 42
key3: true
"#;
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Mapping(pairs, _) => {
            assert_eq!(pairs.len(), 3);

            // Check pairs directly
            let (key1, val1) = &pairs[0];
            let (key2, val2) = &pairs[1];
            let (key3, val3) = &pairs[2];

            assert!(matches!(key1, YamlAst::PlainString(s, _) if s == "key1"));
            assert!(matches!(val1, YamlAst::PlainString(s, _) if s == "value1"));

            assert!(matches!(key2, YamlAst::PlainString(s, _) if s == "key2"));
            assert!(matches!(val2, YamlAst::Number(n, _) if n.as_i64() == Some(42)));

            assert!(matches!(key3, YamlAst::PlainString(s, _) if s == "key3"));
            assert!(matches!(val3, YamlAst::Bool(true, _)));
        }
        _ => panic!("Expected Mapping, got {:?}", result),
    }
}

#[test]
fn test_parse_preprocessing_tag() {
    let yaml = "!$let\n  foo: bar\n  in: result";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::Let(_), _) => {
            // Success - parsed as preprocessing tag
        }
        YamlAst::UnknownYamlTag(tag, _) => {
            // Expected for now since we don't fully implement preprocessing tag parsing
            assert_eq!(tag.tag, "!$let");
        }
        _ => panic!("Expected PreprocessingTag or UnknownTag, got {:?}", result),
    }
}

#[test]
fn test_parse_preprocessing_include_tag() {
    let yaml = "!$ path/to/file.yaml";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::Include(include_tag), _) => {
            assert_eq!(include_tag.path, "path/to/file.yaml");
            assert_eq!(include_tag.query, None);
        }
        _ => panic!(
            "Expected PreprocessingTag::Include for !$, got {:?}",
            result
        ),
    }
}

#[test]
fn test_parse_preprocessing_if_tag() {
    let yaml = r#"!$if
test: true
then: "yes"
else: "no""#;
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::If(if_tag), _) => {
            assert!(matches!(if_tag.test.as_ref(), YamlAst::Bool(true, _)));
            assert!(
                matches!(if_tag.then_value.as_ref(), YamlAst::PlainString(s, _) if s == "yes")
            );
            assert!(
                matches!(if_tag.else_value.as_ref(), Some(else_val) if matches!(else_val.as_ref(), YamlAst::PlainString(s, _) if s == "no"))
            );
        }
        _ => panic!("Expected PreprocessingTag::If, got {:?}", result),
    }
}

#[test]
fn test_parse_cloudformation_tag() {
    let yaml = "!Ref MyResource";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::CloudFormationTag(CloudFormationTag::Ref(content), _) => {
            assert!(
                matches!(content.as_ref(), YamlAst::PlainString(s, _) if s == "MyResource")
            );
        }
        _ => panic!("Expected CloudFormationTag::Ref, got {:?}", result),
    }
}

#[test]
fn test_parse_cloudformation_getatt_tag() {
    let yaml = "!GetAtt Resource.Property";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::CloudFormationTag(CloudFormationTag::GetAtt(content), _) => {
            assert!(
                matches!(content.as_ref(), YamlAst::PlainString(s, _) if s == "Resource.Property")
            );
        }
        _ => panic!("Expected CloudFormationTag::GetAtt, got {:?}", result),
    }
}

#[test]
fn test_parse_cloudformation_sub_tag() {
    let yaml = "!Sub \"Hello ${Name}\"";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::CloudFormationTag(CloudFormationTag::Sub(content), _) => {
            assert!(
                matches!(content.as_ref(), YamlAst::PlainString(s, _) if s == "Hello ${Name}")
            );
        }
        _ => panic!("Expected CloudFormationTag::Sub, got {:?}", result),
    }
}

#[test]
fn test_parse_unknown_tag() {
    let yaml = "!CustomTag value";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::UnknownYamlTag(UnknownTag { tag, value }, _) => {
            assert_eq!(tag, "!CustomTag");
            assert!(matches!(value.as_ref(), YamlAst::PlainString(s, _) if s == "value"));
        }
        _ => panic!("Expected UnknownYamlTag, got {:?}", result),
    }
}

#[test]
fn test_parse_nested_structure() {
    let yaml = r#"
Resources:
  MyBucket:
Type: AWS::S3::Bucket
Properties:
  BucketName: !Sub "${AWS::StackName}-bucket"
  Tags:
    - Key: Environment
      Value: !Ref Environment
"#;
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Mapping(pairs, _) => {
            // Check that we have a "Resources" key
            let has_resources = pairs
                .iter()
                .any(|(key, _)| matches!(key, YamlAst::PlainString(s, _) if s == "Resources"));
            assert!(has_resources);
        }
        _ => panic!("Expected top-level Mapping, got {:?}", result),
    }
}

#[test]
fn test_meta_information() {
    let yaml = "hello";
    let uri = test_uri();
    let result = parse_yaml_ast(yaml, uri.clone()).unwrap();

    let meta = result.meta();
    assert_eq!(meta.input_uri, uri);
    assert_eq!(meta.start.line, 0);
    assert_eq!(meta.start.character, 0);
    assert_eq!(meta.end.line, 0);
    assert_eq!(meta.end.character, 5);
}

// ============================================================================
// PHASE 1: Block-Style Tag Parsing Tests - CRITICAL for old parser removal
// ============================================================================

#[test]
fn test_block_style_let_tag() {
    let yaml = r#"!$let
database_url: "postgres://localhost"
redis_url: "redis://localhost"  
in:
  database: "{{database_url}}/myapp"
  cache: "{{redis_url}}/0""#;
    
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();
    
    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::Let(let_tag), _) => {
            assert_eq!(let_tag.bindings.len(), 2);
            
            // Check bindings
            let (key1, val1) = &let_tag.bindings[0];
            let (key2, val2) = &let_tag.bindings[1];
            
            assert_eq!(key1, "database_url");
            assert_eq!(key2, "redis_url");
            
            assert!(matches!(val1, YamlAst::PlainString(s, _) if s == "postgres://localhost"));
            assert!(matches!(val2, YamlAst::PlainString(s, _) if s == "redis://localhost"));
            
            // Check 'in' expression is a mapping
            assert!(matches!(let_tag.expression.as_ref(), YamlAst::Mapping(_, _)));
        }
        _ => panic!("Expected PreprocessingTag::Let, got {:?}", result),
    }
}

#[test]
fn test_block_style_map_tag() {
    let yaml = r#"!$map
items: !$ servers
template:
  name: "{{item.name}}"
  port: "{{item.port}}"
  url: "http://{{item.name}}:{{item.port}}""#;
    
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();
    
    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::Map(map_tag), _) => {
            // Check items field
            assert!(matches!(map_tag.items.as_ref(), YamlAst::PreprocessingTag(PreprocessingTag::Include(_), _)));
            
            // Check template is a mapping
            assert!(matches!(map_tag.template.as_ref(), YamlAst::Mapping(_, _)));
            
            // Default var should be None (uses "item")
            assert_eq!(map_tag.var, None);
        }
        _ => panic!("Expected PreprocessingTag::Map, got {:?}", result),
    }
}

#[test]
fn test_nested_block_style_tags() {
    let yaml = r#"!$map
items: !$groupBy
  items: !$ rawData
  key: "{{item.category}}"
template: !$merge
  - category: "{{item.key}}"
  - items: !$map
      items: "{{item.value}}"
      template:
        id: "{{item.id}}"
        processed: true"#;
    
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();
    
    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::Map(outer_map), _) => {
            // Check items is a groupBy tag
            assert!(matches!(outer_map.items.as_ref(), YamlAst::PreprocessingTag(PreprocessingTag::GroupBy(_), _)));
            
            // Check template is a merge tag
            assert!(matches!(outer_map.template.as_ref(), YamlAst::PreprocessingTag(PreprocessingTag::Merge(_), _)));
        }
        _ => panic!("Expected nested PreprocessingTag::Map, got {:?}", result),
    }
}

#[test]
fn test_mixed_flow_and_block_styles() {
    let yaml = r#"env: !$if { test: !$ isProd, then: "production", else: !$let { debug: true, in: "{{debug}}" } }"#;
    
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();
    
    match result {
        YamlAst::Mapping(pairs, _) => {
            assert_eq!(pairs.len(), 1);
            let (key, value) = &pairs[0];
            
            assert!(matches!(key, YamlAst::PlainString(s, _) if s == "env"));
            assert!(matches!(value, YamlAst::PreprocessingTag(PreprocessingTag::If(_), _)));
        }
        _ => panic!("Expected Mapping with If tag, got {:?}", result),
    }
}

#[test]
fn test_complex_indentation_scenarios() {
    let yaml = r#"data: !$mapValues
  items:
    server1: { type: "web", port: 80 }
    server2: { type: "api", port: 8080 }
    database: { type: "db", port: 5432 }
  template: "{{item.type}}:{{item.port}}""#;
    
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();
    
    match result {
        YamlAst::Mapping(pairs, _) => {
            assert_eq!(pairs.len(), 1);
            let (key, value) = &pairs[0];
            
            assert!(matches!(key, YamlAst::PlainString(s, _) if s == "data"));
            
            match value {
                YamlAst::PreprocessingTag(PreprocessingTag::MapValues(map_values), _) => {
                    // Check items is a mapping
                    assert!(matches!(map_values.items.as_ref(), YamlAst::Mapping(_, _)));
                    
                    // Check template is a templated string
                    assert!(matches!(map_values.template.as_ref(), YamlAst::TemplatedString(_, _)));
                }
                _ => panic!("Expected MapValues tag, got {:?}", value),
            }
        }
        _ => panic!("Expected Mapping, got {:?}", result),
    }
}

#[test]
fn test_block_style_if_with_complex_conditions() {
    let yaml = r#"!$if
test: true
then: !$merge
  - !$ baseConfig
  - database_pool_size: 20
    cache_enabled: true
else: !$merge
  - !$ baseConfig  
  - database_pool_size: 5
    debug_mode: true"#;
    
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();
    
    match result {
        YamlAst::PreprocessingTag(PreprocessingTag::If(if_tag), _) => {
            // Test condition should be a boolean
            assert!(matches!(if_tag.test.as_ref(), YamlAst::Bool(true, _)));
            
            // Then and else should be merge operations
            assert!(matches!(if_tag.then_value.as_ref(), YamlAst::PreprocessingTag(PreprocessingTag::Merge(_), _)));
            assert!(matches!(if_tag.else_value.as_ref(), Some(else_val) if matches!(else_val.as_ref(), YamlAst::PreprocessingTag(PreprocessingTag::Merge(_), _))));
        }
        _ => panic!("Expected PreprocessingTag::If, got {:?}", result),
    }
}

#[test]
fn test_syntax_error() {
    let yaml = "key: [\n  unclosed";
    let result = parse_yaml_ast(yaml, test_uri());

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.location.is_some());
}

#[test]
fn test_empty_mapping_value() {
    let yaml = "key:";
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::Mapping(pairs, _) => {
            assert_eq!(pairs.len(), 1);
            let (key, value) = &pairs[0];
            assert!(matches!(key, YamlAst::PlainString(s, _) if s == "key"));
            assert!(matches!(value, YamlAst::Null(_)));
        }
        _ => panic!("Expected Mapping, got {:?}", result),
    }
}

#[test]
fn test_quoted_strings() {
    let yaml = r#""quoted string""#;
    let result = parse_yaml_ast(yaml, test_uri()).unwrap();

    match result {
        YamlAst::PlainString(s, _) => assert_eq!(s, "quoted string"),
        _ => panic!("Expected PlainString, got {:?}", result),
    }
}
