//! Debug array path parsing and navigation

use iidy::yaml::tree_sitter_location::*;

#[test]
fn debug_array_path_parsing() {
    let path = ["ListOperations[2]", "operation"];
    let (clean_path, indices) = parse_path_with_indices(&path);
    
    println!("Original path: {:?}", path);
    println!("Clean path: {:?}", clean_path);
    println!("Array indices: {:?}", indices);
    
    // This should be:
    // Clean path: ["ListOperations", "operation"]
    // Array indices: [2]
}

#[test]
fn debug_array_navigation() {
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
    
    // Find ListOperations
    if let Some(list_node) = find_yaml_node_by_path(&root, &["ListOperations"], yaml_source) {
        println!("Found ListOperations: kind={}", list_node.kind());
        print_node_detailed(&list_node, yaml_source, 0, 4);
        
        // Navigate to the block_sequence
        let mut cursor = list_node.walk();
        for child in list_node.named_children(&mut cursor) {
            if child.kind() == "block_node" {
                let mut inner_cursor = child.walk();
                for inner_child in child.named_children(&mut inner_cursor) {
                    if inner_child.kind() == "block_sequence" {
                        println!("\nFound block_sequence:");
                        print_node_detailed(&inner_child, yaml_source, 0, 3);
                        
                        // Try to find the third element (index 2)
                        if let Some(third_element) = find_array_element(&inner_child, 2, yaml_source) {
                            println!("\nFound third element: kind={}", third_element.kind());
                            print_node_detailed(&third_element, yaml_source, 0, 3);
                        } else {
                            println!("\nCould not find third element");
                        }
                        break;
                    }
                }
                break;
            }
        }
    } else {
        println!("Could not find ListOperations");
    }
}

fn print_node_detailed(node: &tree_sitter::Node, source: &str, depth: usize, max_depth: usize) {
    let indent = "  ".repeat(depth);
    let text = &source[node.start_byte()..node.end_byte()];
    let text_preview = if text.len() > 40 {
        format!("{}...", &text[..37])
    } else {
        text.replace('\n', "\\n")
    };
    
    println!("{}Kind: {} | Text: {:?}", indent, node.kind(), text_preview);
    
    if depth < max_depth {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node_detailed(&child, source, depth + 1, max_depth);
        }
    }
}