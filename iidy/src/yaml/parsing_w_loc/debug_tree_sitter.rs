use tree_sitter::{Node, Parser, Tree};
use tree_sitter_yaml::LANGUAGE;

/// Debug function to print tree-sitter structure for YAML
pub fn debug_tree_sitter_structure(source: &str, title: &str) {
    println!("\n=== {} ===", title);
    println!("Source:\n{}", source);
    
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE.into()).unwrap();
    
    if let Some(tree) = parser.parse(source, None) {
        print_node_structure(tree.root_node(), source.as_bytes(), 0);
    } else {
        println!("Failed to parse");
    }
}

fn print_node_structure(node: Node, source: &[u8], indent: usize) {
    let indent_str = "  ".repeat(indent);
    let node_text = node.utf8_text(source).unwrap_or("<invalid utf8>");
    let node_text_short = if node_text.len() > 50 {
        format!("{}...", &node_text[..50])
    } else {
        node_text.to_string()
    };
    
    println!("{}{}@{}:{}-{}:{} = {:?}", 
        indent_str, 
        node.kind(),
        node.start_position().row, node.start_position().column,
        node.end_position().row, node.end_position().column,
        node_text_short
    );
    
    // Print child nodes
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            print_node_structure(child, source, indent + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn debug_block_style_if_tag() {
        let yaml = r#"log_level: !$if
  test: !$eq ["production", "staging"]
  then: "WARN"
  else: "DEBUG""#;
        
        debug_tree_sitter_structure(yaml, "Block-style !$if tag");
    }
    
    #[test]
    fn debug_flow_style_tag() {
        let yaml = r#"stack_name: !$join ["-", ["app", "prod"]]"#;
        
        debug_tree_sitter_structure(yaml, "Flow-style !$join tag");
    }
    
    #[test]
    fn debug_mapvalues_tag() {
        let yaml = r#"user_roles: !$mapValues
  items: !$ user_data
  template: "admin""#;
        
        debug_tree_sitter_structure(yaml, "Block-style !$mapValues tag");
    }
    
    #[test]
    fn debug_nested_not_tag() {
        let yaml = r#"test: !$not
    test: !$eq ["prod", "staging"]"#;
        
        debug_tree_sitter_structure(yaml, "Nested !$not with !$eq");
    }
    
    #[test]
    fn debug_groupby_document_structure() {
        if let Ok(content) = std::fs::read_to_string("example-templates/yaml-iidy-syntax/groupby.yaml") {
            println!("groupby.yaml has {} lines", content.lines().count());
            
            // Check if tree-sitter sees any errors
            use tree_sitter::{Parser};
            use tree_sitter_yaml::LANGUAGE;
            
            let mut parser = Parser::new();
            parser.set_language(&LANGUAGE.into()).unwrap();
            
            if let Some(tree) = parser.parse(&content, None) {
                let root = tree.root_node();
                println!("Tree-sitter root: {} ({}:{} to {}:{})", 
                    root.kind(),
                    root.start_position().row, root.start_position().column,
                    root.end_position().row, root.end_position().column
                );
                
                if root.has_error() {
                    println!("⚠ Tree-sitter reports syntax errors");
                } else {
                    println!("✓ Tree-sitter parses without syntax errors");
                    println!("Root has {} children", root.named_child_count());
                    
                    // Show first few children types
                    for i in 0..std::cmp::min(5, root.named_child_count()) {
                        if let Some(child) = root.named_child(i) {
                            println!("  Child {}: {} ({}:{})", i, child.kind(), 
                                child.start_position().row, child.start_position().column);
                        }
                    }
                }
            } else {
                println!("✗ Tree-sitter failed to parse");
            }
        }
    }
}