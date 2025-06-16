//! Conversion utilities for converting between location-aware and original AST types
//!
//! This module provides functionality to convert from the location-aware YamlAst
//! (with SrcMeta) to the original YamlAst (without location information).

use crate::yaml::parsing::ast as original;
use super::ast as with_location;
use super::parser::parse_yaml_ast;
use super::error::ParseDiagnostics;
use super::parse_yaml_ast_with_diagnostics;
use url::Url;

/// Convert from location-aware YamlAst to original YamlAst
/// 
/// This function strips away all SrcMeta location information while preserving
/// the identical tree structure. The resulting AST should be functionally
/// equivalent to what would have been parsed by the original parser.
pub fn to_original_ast(ast: &with_location::YamlAst) -> original::YamlAst {
    match ast {
        with_location::YamlAst::Null(_) => original::YamlAst::Null,
        
        with_location::YamlAst::Bool(b, _) => original::YamlAst::Bool(*b),
        
        with_location::YamlAst::Number(n, _) => original::YamlAst::Number(n.clone()),
        
        with_location::YamlAst::PlainString(s, _) => original::YamlAst::PlainString(s.clone()),
        
        with_location::YamlAst::TemplatedString(s, _) => original::YamlAst::TemplatedString(s.clone()),
        
        with_location::YamlAst::Sequence(items, _) => {
            let converted_items = items.iter().map(to_original_ast).collect();
            original::YamlAst::Sequence(converted_items)
        },
        
        with_location::YamlAst::Mapping(pairs, _) => {
            let converted_pairs = pairs.iter()
                .map(|(key, value)| (to_original_ast(key), to_original_ast(value)))
                .collect();
            original::YamlAst::Mapping(converted_pairs)
        },
        
        with_location::YamlAst::PreprocessingTag(tag, _) => {
            let converted_tag = convert_preprocessing_tag(tag);
            original::YamlAst::PreprocessingTag(converted_tag)
        },
        
        with_location::YamlAst::CloudFormationTag(tag, _) => {
            let converted_tag = convert_cloudformation_tag(tag);
            original::YamlAst::CloudFormationTag(converted_tag)
        },
        
        with_location::YamlAst::UnknownYamlTag(tag, _) => {
            let converted_tag = original::UnknownTag {
                tag: tag.tag.clone(),
                value: Box::new(to_original_ast(&tag.value)),
            };
            original::YamlAst::UnknownYamlTag(converted_tag)
        },
        
        with_location::YamlAst::ImportedDocument(doc, _) => {
            let converted_doc = original::ImportedDocumentNode {
                source_uri: doc.source_uri.clone(),
                import_key: doc.import_key.clone(),
                content: Box::new(to_original_ast(&doc.content)),
                metadata: original::ImportMetadata {
                    content_hash: doc.metadata.content_hash.clone(),
                    imported_at: doc.metadata.imported_at,
                    import_type: doc.metadata.import_type.clone(),
                },
            };
            original::YamlAst::ImportedDocument(converted_doc)
        },
    }
}

/// Convert CloudFormation tag from location-aware to original
fn convert_cloudformation_tag(tag: &with_location::CloudFormationTag) -> original::CloudFormationTag {
    match tag {
        with_location::CloudFormationTag::Ref(content) => {
            original::CloudFormationTag::Ref(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Sub(content) => {
            original::CloudFormationTag::Sub(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::GetAtt(content) => {
            original::CloudFormationTag::GetAtt(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Join(content) => {
            original::CloudFormationTag::Join(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Select(content) => {
            original::CloudFormationTag::Select(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Split(content) => {
            original::CloudFormationTag::Split(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Base64(content) => {
            original::CloudFormationTag::Base64(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::GetAZs(content) => {
            original::CloudFormationTag::GetAZs(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::ImportValue(content) => {
            original::CloudFormationTag::ImportValue(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::FindInMap(content) => {
            original::CloudFormationTag::FindInMap(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Cidr(content) => {
            original::CloudFormationTag::Cidr(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Length(content) => {
            original::CloudFormationTag::Length(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::ToJsonString(content) => {
            original::CloudFormationTag::ToJsonString(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Transform(content) => {
            original::CloudFormationTag::Transform(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::ForEach(content) => {
            original::CloudFormationTag::ForEach(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::If(content) => {
            original::CloudFormationTag::If(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Equals(content) => {
            original::CloudFormationTag::Equals(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::And(content) => {
            original::CloudFormationTag::And(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Or(content) => {
            original::CloudFormationTag::Or(Box::new(to_original_ast(content)))
        },
        with_location::CloudFormationTag::Not(content) => {
            original::CloudFormationTag::Not(Box::new(to_original_ast(content)))
        },
    }
}

/// Convert preprocessing tag from location-aware to original
fn convert_preprocessing_tag(tag: &with_location::PreprocessingTag) -> original::PreprocessingTag {
    match tag {
        with_location::PreprocessingTag::Include(include_tag) => {
            let converted = original::IncludeTag {
                path: include_tag.path.clone(),
                query: include_tag.query.clone(),
            };
            original::PreprocessingTag::Include(converted)
        },
        with_location::PreprocessingTag::If(if_tag) => {
            let converted = original::IfTag {
                test: Box::new(to_original_ast(&if_tag.test)),
                then_value: Box::new(to_original_ast(&if_tag.then_value)),
                else_value: if_tag.else_value.as_ref().map(|v| Box::new(to_original_ast(v))),
            };
            original::PreprocessingTag::If(converted)
        },
        with_location::PreprocessingTag::Map(map_tag) => {
            let converted = original::MapTag {
                items: Box::new(to_original_ast(&map_tag.items)),
                template: Box::new(to_original_ast(&map_tag.template)),
                var: map_tag.var.clone(),
                filter: map_tag.filter.as_ref().map(|f| Box::new(to_original_ast(f))),
            };
            original::PreprocessingTag::Map(converted)
        },
        with_location::PreprocessingTag::Merge(merge_tag) => {
            let converted = original::MergeTag {
                sources: merge_tag.sources.iter().map(to_original_ast).collect(),
            };
            original::PreprocessingTag::Merge(converted)
        },
        with_location::PreprocessingTag::Concat(concat_tag) => {
            let converted = original::ConcatTag {
                sources: concat_tag.sources.iter().map(to_original_ast).collect(),
            };
            original::PreprocessingTag::Concat(converted)
        },
        with_location::PreprocessingTag::Let(let_tag) => {
            let converted = original::LetTag {
                bindings: let_tag.bindings.iter()
                    .map(|(k, v)| (k.clone(), to_original_ast(v)))
                    .collect(),
                expression: Box::new(to_original_ast(&let_tag.expression)),
            };
            original::PreprocessingTag::Let(converted)
        },
        with_location::PreprocessingTag::Eq(eq_tag) => {
            let converted = original::EqTag {
                left: Box::new(to_original_ast(&eq_tag.left)),
                right: Box::new(to_original_ast(&eq_tag.right)),
            };
            original::PreprocessingTag::Eq(converted)
        },
        with_location::PreprocessingTag::Not(not_tag) => {
            let converted = original::NotTag {
                expression: Box::new(to_original_ast(&not_tag.expression)),
            };
            original::PreprocessingTag::Not(converted)
        },
        with_location::PreprocessingTag::Split(split_tag) => {
            let converted = original::SplitTag {
                delimiter: Box::new(to_original_ast(&split_tag.delimiter)),
                string: Box::new(to_original_ast(&split_tag.string)),
            };
            original::PreprocessingTag::Split(converted)
        },
        with_location::PreprocessingTag::Join(join_tag) => {
            let converted = original::JoinTag {
                delimiter: Box::new(to_original_ast(&join_tag.delimiter)),
                array: Box::new(to_original_ast(&join_tag.array)),
            };
            original::PreprocessingTag::Join(converted)
        },
        with_location::PreprocessingTag::ConcatMap(concat_map_tag) => {
            let converted = original::ConcatMapTag {
                items: Box::new(to_original_ast(&concat_map_tag.items)),
                template: Box::new(to_original_ast(&concat_map_tag.template)),
                var: concat_map_tag.var.clone(),
                filter: concat_map_tag.filter.as_ref().map(|f| Box::new(to_original_ast(f))),
            };
            original::PreprocessingTag::ConcatMap(converted)
        },
        with_location::PreprocessingTag::MergeMap(merge_map_tag) => {
            let converted = original::MergeMapTag {
                items: Box::new(to_original_ast(&merge_map_tag.items)),
                template: Box::new(to_original_ast(&merge_map_tag.template)),
                var: merge_map_tag.var.clone(),
            };
            original::PreprocessingTag::MergeMap(converted)
        },
        with_location::PreprocessingTag::MapListToHash(map_list_to_hash_tag) => {
            let converted = original::MapListToHashTag {
                items: Box::new(to_original_ast(&map_list_to_hash_tag.items)),
                template: Box::new(to_original_ast(&map_list_to_hash_tag.template)),
                var: map_list_to_hash_tag.var.clone(),
                filter: map_list_to_hash_tag.filter.as_ref().map(|f| Box::new(to_original_ast(f))),
            };
            original::PreprocessingTag::MapListToHash(converted)
        },
        with_location::PreprocessingTag::MapValues(map_values_tag) => {
            let converted = original::MapValuesTag {
                items: Box::new(to_original_ast(&map_values_tag.items)),
                template: Box::new(to_original_ast(&map_values_tag.template)),
                var: map_values_tag.var.clone(),
            };
            original::PreprocessingTag::MapValues(converted)
        },
        with_location::PreprocessingTag::GroupBy(group_by_tag) => {
            let converted = original::GroupByTag {
                items: Box::new(to_original_ast(&group_by_tag.items)),
                key: Box::new(to_original_ast(&group_by_tag.key)),
                var: group_by_tag.var.clone(),
                template: group_by_tag.template.as_ref().map(|t| Box::new(to_original_ast(t))),
            };
            original::PreprocessingTag::GroupBy(converted)
        },
        with_location::PreprocessingTag::FromPairs(from_pairs_tag) => {
            let converted = original::FromPairsTag {
                source: Box::new(to_original_ast(&from_pairs_tag.source)),
            };
            original::PreprocessingTag::FromPairs(converted)
        },
        with_location::PreprocessingTag::ToYamlString(to_yaml_string_tag) => {
            let converted = original::ToYamlStringTag {
                data: Box::new(to_original_ast(&to_yaml_string_tag.data)),
            };
            original::PreprocessingTag::ToYamlString(converted)
        },
        with_location::PreprocessingTag::ParseYaml(parse_yaml_tag) => {
            let converted = original::ParseYamlTag {
                yaml_string: Box::new(to_original_ast(&parse_yaml_tag.yaml_string)),
            };
            original::PreprocessingTag::ParseYaml(converted)
        },
        with_location::PreprocessingTag::ToJsonString(to_json_string_tag) => {
            let converted = original::ToJsonStringTag {
                data: Box::new(to_original_ast(&to_json_string_tag.data)),
            };
            original::PreprocessingTag::ToJsonString(converted)
        },
        with_location::PreprocessingTag::ParseJson(parse_json_tag) => {
            let converted = original::ParseJsonTag {
                json_string: Box::new(to_original_ast(&parse_json_tag.json_string)),
            };
            original::PreprocessingTag::ParseJson(converted)
        },
        with_location::PreprocessingTag::Escape(escape_tag) => {
            let converted = original::EscapeTag {
                content: Box::new(to_original_ast(&escape_tag.content)),
            };
            original::PreprocessingTag::Escape(converted)
        },
    }
}

/// Drop-in replacement for `parser::parse_yaml_with_custom_tags_from_file`
/// 
/// This function provides the same interface as the original parser but uses
/// the new tree-sitter parser with location tracking internally, then converts
/// the result to the original AST format for full compatibility.
pub fn parse_and_convert_to_original(source: &str, uri_str: &str) -> anyhow::Result<original::YamlAst> {
    // Try parsing as URI first, fallback to treating as file path
    let uri = match Url::parse(uri_str) {
        Ok(uri) => uri,
        Err(_) => {
            // If it's not a valid URI, try treating it as a file path
            match Url::from_file_path(uri_str) {
                Ok(uri) => uri,
                Err(_) => {
                    // As last resort, create a basic file URI
                    Url::parse(&format!("file://{}", uri_str))
                        .map_err(|e| anyhow::anyhow!("Cannot create URI from '{}': {}", uri_str, e))?
                }
            }
        }
    };
    
    let with_location_ast = parse_yaml_ast(source, uri)
        .map_err(|e| anyhow::anyhow!("{}", e.message))?;
    
    Ok(to_original_ast(&with_location_ast))
}

/// New diagnostic API for convert module
pub fn parse_and_convert_to_original_with_diagnostics(source: &str, uri_str: &str) -> Result<ParseDiagnostics, anyhow::Error> {
    let uri = create_uri_from_string(uri_str)?;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, uri);
    Ok(diagnostics)
}

/// Validate YAML without conversion (useful for linting)
pub fn validate_yaml_only(source: &str, uri_str: &str) -> Result<ParseDiagnostics, anyhow::Error> {
    parse_and_convert_to_original_with_diagnostics(source, uri_str)
}

fn create_uri_from_string(uri_str: &str) -> anyhow::Result<Url> {
    match Url::parse(uri_str) {
        Ok(uri) => Ok(uri),
        Err(_) => {
            match Url::from_file_path(uri_str) {
                Ok(uri) => Ok(uri),
                Err(_) => {
                    Url::parse(&format!("file://{}", uri_str))
                        .map_err(|e| anyhow::anyhow!("Cannot create URI from '{}': {}", uri_str, e))
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::parsing_w_loc::parse_yaml_ast;
    use url::Url;

    fn test_uri() -> Url {
        Url::parse("file:///test.yaml").unwrap()
    }

    #[test]
    fn test_convert_simple_scalar() {
        let yaml = "hello world";
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        match original {
            original::YamlAst::PlainString(s) => assert_eq!(s, "hello world"),
            _ => panic!("Expected PlainString, got {:?}", original),
        }
    }

    #[test]
    fn test_convert_boolean() {
        let yaml = "true";
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        match original {
            original::YamlAst::Bool(b) => assert!(b),
            _ => panic!("Expected Bool, got {:?}", original),
        }
    }

    #[test]
    fn test_convert_sequence() {
        let yaml = r#"
- item1
- item2
- 42
"#;
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        match original {
            original::YamlAst::Sequence(items) => {
                assert_eq!(items.len(), 3);
                assert!(matches!(items[0], original::YamlAst::PlainString(ref s) if s == "item1"));
                assert!(matches!(items[1], original::YamlAst::PlainString(ref s) if s == "item2"));
                assert!(matches!(items[2], original::YamlAst::Number(_)));
            }
            _ => panic!("Expected Sequence, got {:?}", original),
        }
    }

    #[test]
    fn test_convert_mapping() {
        let yaml = r#"
key1: value1
key2: 42
"#;
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        match original {
            original::YamlAst::Mapping(pairs) => {
                assert_eq!(pairs.len(), 2);
                
                let (key1, val1) = &pairs[0];
                let (key2, val2) = &pairs[1];
                
                assert!(matches!(key1, original::YamlAst::PlainString(s) if s == "key1"));
                assert!(matches!(val1, original::YamlAst::PlainString(s) if s == "value1"));
                
                assert!(matches!(key2, original::YamlAst::PlainString(s) if s == "key2"));
                assert!(matches!(val2, original::YamlAst::Number(_)));
            }
            _ => panic!("Expected Mapping, got {:?}", original),
        }
    }

    #[test]
    fn test_convert_cloudformation_tag() {
        let yaml = "!Ref MyResource";
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        match original {
            original::YamlAst::CloudFormationTag(original::CloudFormationTag::Ref(content)) => {
                assert!(matches!(content.as_ref(), original::YamlAst::PlainString(s) if s == "MyResource"));
            }
            _ => panic!("Expected CloudFormationTag::Ref, got {:?}", original),
        }
    }

    #[test]
    fn test_convert_unknown_tag() {
        let yaml = "!CustomTag value";
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        match original {
            original::YamlAst::UnknownYamlTag(tag) => {
                assert_eq!(tag.tag, "!CustomTag");
                assert!(matches!(tag.value.as_ref(), original::YamlAst::PlainString(s) if s == "value"));
            }
            _ => panic!("Expected UnknownYamlTag, got {:?}", original),
        }
    }

    #[test]
    fn test_convert_nested_structure() {
        let yaml = r#"
Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Ref BucketNameParam
"#;
        let with_loc = parse_yaml_ast(yaml, test_uri()).unwrap();
        let original = to_original_ast(&with_loc);
        
        // Just verify it's a mapping and contains expected structure
        match original {
            original::YamlAst::Mapping(_) => {
                // Successfully converted nested structure
            }
            _ => panic!("Expected top-level Mapping, got {:?}", original),
        }
    }

    #[test]
    fn test_parse_and_convert_convenience_function() {
        let yaml = r#"
name: test
value: !Ref Parameter
items:
  - item1
  - item2
"#;
        let result = parse_and_convert_to_original(yaml, test_uri().as_str()).unwrap();
        
        match result {
            original::YamlAst::Mapping(pairs) => {
                assert_eq!(pairs.len(), 3);
                
                // Verify we have the expected keys
                let keys: Vec<_> = pairs.iter()
                    .filter_map(|(k, _)| {
                        if let original::YamlAst::PlainString(s) = k {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                
                assert!(keys.contains(&"name"));
                assert!(keys.contains(&"value"));
                assert!(keys.contains(&"items"));
                
                // Verify the CloudFormation tag was preserved
                let value_pair = pairs.iter().find(|(k, _)| {
                    matches!(k, original::YamlAst::PlainString(s) if s == "value")
                }).unwrap();
                
                assert!(matches!(value_pair.1, original::YamlAst::CloudFormationTag(original::CloudFormationTag::Ref(_))));
            }
            _ => panic!("Expected Mapping, got {:?}", result),
        }
    }
}