//! Debug test to see how YAML tags are represented in tree-sitter

use iidy::yaml::tree_sitter_location::*;

#[test]
fn debug_yaml_tag_structure() {
    let yaml_source = r#"BucketName: !$if
  test: true
  then: "prod-bucket"
  else: "dev-bucket""#;

    let mut parser = create_yaml_parser().unwrap();
    let tree = parse_yaml_source(&mut parser, yaml_source).unwrap();
    let root = tree.root_node();
    
    println!("YAML with tag:");
    print_node_deep(&root, yaml_source, 0, 10);
}

#[test]
fn debug_simple_tag() {
    let yaml_source = r#"value: !mytag "hello world""#;

    let mut parser = create_yaml_parser().unwrap();
    let tree = parse_yaml_source(&mut parser, yaml_source).unwrap();
    let root = tree.root_node();
    
    println!("Simple tag YAML:");
    print_node_deep(&root, yaml_source, 0, 10);
}

fn print_node_deep(node: &tree_sitter::Node, source: &str, depth: usize, max_depth: usize) {
    let indent = "  ".repeat(depth);
    let text = &source[node.start_byte()..node.end_byte()];
    let text_preview = if text.len() > 50 {
        format!("{}...", &text[..47])
    } else {
        text.replace('\n', "\\n")
    };
    
    println!("{}Kind: {} | Pos: {}:{} | Text: {:?}", 
             indent, node.kind(), node.start_position().row + 1, node.start_position().column + 1, text_preview);
    
    if depth < max_depth {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node_deep(&child, source, depth + 1, max_depth);
        }
    }
}