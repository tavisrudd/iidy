use std::path::Path;
use url::Url;

use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::compare_asts;

#[test]
fn test_specific_failing_cases() {
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    // Test case from include-equivalence.yaml that might be failing
    let test_cases = vec![
        // Object form with query from include-equivalence.yaml
        (
            "include object form with query",
            r#"
explicit_short: !$
  path: config.database
  query: "host,port"
"#
        ),
        // Dynamic bracket notation
        (
            "dynamic bracket notation",
            r#"
dynamic_short: !$ config[environment]
"#
        ),
        // From array-syntax.yaml - parse tags with array
        (
            "parseYaml with array",
            r#"
parse_yaml_array: !$parseYaml ["key: parsed_value\nnumber: 123"]
"#
        ),
        (
            "parseJson with array",
            r#"
parse_json_array: !$parseJson ['{"parsed": true, "count": 456}']
"#
        ),
    ];
    
    for (name, yaml) in test_cases {
        println!("\n=== Testing {} ===", name);
        
        match parse_yaml_ast(yaml, uri.clone()) {
            Ok(tree_sitter_ast) => {
                let converted = to_original_ast(&tree_sitter_ast);
                
                match original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str()) {
                    Ok(original_ast) => {
                        if compare_asts(&converted, &original_ast, Path::new("debug_test")).is_ok() {
                            println!("✓ {} compatible", name);
                        } else {
                            println!("✗ {} incompatible", name);
                            println!("Converted: {:#?}", converted);
                            println!("Original: {:#?}", original_ast);
                        }
                    }
                    Err(e) => {
                        println!("✗ Original parser failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Tree-sitter parser failed: {}", e.message);
            }
        }
    }
}