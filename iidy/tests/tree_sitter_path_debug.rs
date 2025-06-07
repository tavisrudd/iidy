//! Debug test to see what nodes we get when following paths

use iidy::yaml::tree_sitter_location::*;

#[test]
fn debug_path_finding() {
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
    
    // Try to find the path step by step
    println!("=== Finding Resources ===");
    if let Some(resources_node) = find_yaml_node_by_path(&root, &["Resources"], yaml_source) {
        println!("Found Resources node: kind={}, text={:?}", 
                 resources_node.kind(), 
                 &yaml_source[resources_node.start_byte()..resources_node.end_byte().min(resources_node.start_byte() + 50)]);
        print_node_simple(&resources_node, yaml_source, 0, 3);
    } else {
        println!("Could not find Resources");
    }
    
    println!("\n=== Finding Resources.MyBucket ===");
    if let Some(bucket_node) = find_yaml_node_by_path(&root, &["Resources", "MyBucket"], yaml_source) {
        println!("Found MyBucket node: kind={}, text={:?}", 
                 bucket_node.kind(), 
                 &yaml_source[bucket_node.start_byte()..bucket_node.end_byte().min(bucket_node.start_byte() + 50)]);
        print_node_simple(&bucket_node, yaml_source, 0, 3);
    } else {
        println!("Could not find Resources.MyBucket");
    }
    
    println!("\n=== Finding Resources.MyBucket.Properties.BucketName ===");
    if let Some(bucket_name_node) = find_yaml_node_by_path(&root, &["Resources", "MyBucket", "Properties", "BucketName"], yaml_source) {
        println!("Found BucketName node: kind={}, text={:?}", 
                 bucket_name_node.kind(), 
                 &yaml_source[bucket_name_node.start_byte()..bucket_name_node.end_byte().min(bucket_name_node.start_byte() + 100)]);
        print_node_simple(&bucket_name_node, yaml_source, 0, 4);
        
        // Now try to find the tag
        println!("\n=== Looking for !$if tag in BucketName node ===");
        if let Some(tag_node) = find_tag_in_node(&bucket_name_node, "!$if", yaml_source) {
            println!("Found !$if tag: kind={}, pos={}:{}, text={:?}", 
                     tag_node.kind(),
                     tag_node.start_position().row + 1,
                     tag_node.start_position().column + 1,
                     &yaml_source[tag_node.start_byte()..tag_node.end_byte()]);
        } else {
            println!("Could not find !$if tag in BucketName node");
        }
    } else {
        println!("Could not find Resources.MyBucket.Properties.BucketName");
    }
}

fn print_node_simple(node: &tree_sitter::Node, source: &str, depth: usize, max_depth: usize) {
    let indent = "  ".repeat(depth);
    let text = &source[node.start_byte()..node.end_byte()];
    let text_preview = if text.len() > 30 {
        format!("{}...", &text[..27])
    } else {
        text.replace('\n', "\\n")
    };
    
    println!("{}Kind: {} | Text: {:?}", indent, node.kind(), text_preview);
    
    if depth < max_depth {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            print_node_simple(&child, source, depth + 1, max_depth);
        }
    }
}