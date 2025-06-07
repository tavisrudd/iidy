//! Debug test to explore tree-sitter YAML AST structure

use iidy::yaml::tree_sitter_location::*;

#[test]
fn debug_tree_sitter_yaml_structure() {
    let yaml_source = r#"
Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !$if
        test: true
        then: "prod-bucket"
        else: "dev-bucket"
"#;

    let mut parser = create_yaml_parser().unwrap();
    let tree = parse_yaml_source(&mut parser, yaml_source).unwrap();
    let root = tree.root_node();
    
    println!("Root node kind: {}", root.kind());
    println!("Root node text: {}", &yaml_source[root.start_byte()..root.end_byte()]);
    
    // Print all children
    print_node(&root, yaml_source, 0);
}

#[test]
fn debug_array_yaml_structure() {
    let yaml_source = r#"
ListOperations:
  - operation: !$map
      items: [1, 2]
      template: "{{item}}"
  - operation: !$if
      test: true
      then: "ok"
  - operation: !$map
      items: [a, b]
      # missing template field
"#;

    let mut parser = create_yaml_parser().unwrap();
    let tree = parse_yaml_source(&mut parser, yaml_source).unwrap();
    let root = tree.root_node();
    
    println!("Array YAML Root node kind: {}", root.kind());
    
    // Print all children
    print_node(&root, yaml_source, 0);
}

fn print_node(node: &tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = &source[node.start_byte()..node.end_byte()];
    let text_preview = if text.len() > 50 {
        format!("{}...", &text[..47])
    } else {
        text.to_string()
    };
    
    println!("{}Kind: {} | Text: {:?}", indent, node.kind(), text_preview);
    
    if depth < 6 { // Limit depth to avoid too much output
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node(&child, source, depth + 1);
        }
    }
}