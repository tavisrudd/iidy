//! Tree-sitter based YAML position finding
//! 
//! This module provides precise YAML tag and node position finding using tree-sitter's
//! AST parsing capabilities, offering superior accuracy for complex YAML structures.

use anyhow::{anyhow, Result};
use tree_sitter::{Node, Parser, Point};

/// Initialize a tree-sitter parser for YAML
pub fn create_yaml_parser() -> Result<Parser> {
    let mut parser = Parser::new();
    let language: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| anyhow!("Failed to set YAML language for tree-sitter"))?;
    Ok(parser)
}

/// Parse YAML source and return the root node
pub fn parse_yaml_source(parser: &mut Parser, source: &str) -> Result<tree_sitter::Tree> {
    parser
        .parse(source, None)
        .ok_or_else(|| anyhow!("Failed to parse YAML with tree-sitter"))
}

/// Find a child YAML node by key name within a mapping
pub fn find_child_by_key<'a>(
    mapping_node: &Node<'a>,
    key_name: &str,
    source: &str,
) -> Option<Node<'a>> {
    let mut cursor = mapping_node.walk();
    
    // Look through all children to find the mapping pair with the right key
    for child in mapping_node.named_children(&mut cursor) {
        match child.kind() {
            "block_mapping_pair" | "flow_mapping_pair" => {
                if let Some(key_node) = child.child_by_field_name("key") {
                    let key_text = &source[key_node.byte_range()];
                    if key_text.trim() == key_name {
                        return Some(child);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Find a YAML node by following a path like ["Resources", "MyBucket", "Properties"]
pub fn find_yaml_node_by_path<'a>(
    root: &Node<'a>,
    path: &[&str],
    source: &str,
) -> Option<Node<'a>> {
    let mut cursor = root.walk();
    let mut current = *root;

    // First, navigate through the tree-sitter wrapper nodes
    // YAML structure is: stream -> document -> block_node -> block_mapping
    match current.kind() {
        "stream" => {
            // Find the document child
            for child in current.named_children(&mut cursor) {
                if child.kind() == "document" {
                    current = child;
                    break;
                }
            }
        }
        _ => {}
    }
    
    // Now navigate through the document to the actual content
    if current.kind() == "document" {
        for child in current.named_children(&mut cursor) {
            if child.kind() == "block_node" {
                current = child;
                break;
            }
        }
    }
    
    // Navigate through block_node to get to the mapping
    if current.kind() == "block_node" {
        for child in current.named_children(&mut cursor) {
            if child.kind() == "block_mapping" {
                current = child;
                break;
            }
        }
    }

    // Now follow the path through the YAML structure
    for &key in path {
        let mut found = false;
        
        // Look through all children to find the mapping pair with the right key
        for child in current.named_children(&mut cursor) {
            match child.kind() {
                "block_mapping_pair" | "flow_mapping_pair" => {
                    if let Some(key_node) = child.child_by_field_name("key") {
                        let key_text = &source[key_node.byte_range()];
                        if key_text.trim() == key {
                            // For the final key in the path, return the mapping pair itself
                            // so we can find tags associated with the value
                            if key == *path.last().unwrap() {
                                current = child;
                                found = true;
                                break;
                            } else if let Some(value_node) = child.child_by_field_name("value") {
                                current = value_node;
                                found = true;
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        
        if !found {
            return None;
        }
        
        // If the current value is a block_node, navigate into its content
        if current.kind() == "block_node" {
            for child in current.named_children(&mut cursor) {
                if matches!(child.kind(), "block_mapping" | "block_sequence") {
                    current = child;
                    break;
                }
            }
        }
    }
    
    Some(current)
}

/// Find a specific array element by index within a YAML sequence
pub fn find_array_element<'a>(
    sequence_node: &Node<'a>,
    index: usize,
    _source: &str,
) -> Option<Node<'a>> {
    let mut cursor = sequence_node.walk();
    let array_elements: Vec<_> = sequence_node
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "block_sequence_item" || n.kind() == "flow_sequence_item")
        .collect();

    if index >= array_elements.len() {
        return None;
    }

    let item_node = array_elements[index];
    
    // Try to get the value from the sequence item, or use the item itself
    item_node
        .child_by_field_name("value")
        .or_else(|| {
            // For block sequence items, the value might be a direct child
            item_node.named_children(&mut item_node.walk()).next()
        })
        .unwrap_or(item_node)
        .into()
}

/// Find a tag within a YAML node (like !$if, !$map, etc.)
pub fn find_tag_in_node<'a>(
    node: &Node<'a>,
    tag_name: &str,
    source: &str,
) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    
    // Check if this node itself has the tag
    if node.kind() == "tag" {
        let tag_text = &source[node.byte_range()];
        if tag_text.trim() == tag_name {
            return Some(*node);
        }
    }
    
    // Check if this node has a tag child
    if let Some(tag_node) = node.child_by_field_name("tag") {
        let tag_text = &source[tag_node.byte_range()];
        if tag_text.trim() == tag_name {
            return Some(tag_node);
        }
    }
    
    // For tagged nodes, check the tag field
    if node.kind() == "tagged_node" {
        if let Some(tag_node) = node.child_by_field_name("tag") {
            let tag_text = &source[tag_node.byte_range()];
            if tag_text.trim() == tag_name {
                return Some(tag_node);
            }
        }
    }
    
    // Recursively search children
    for child in node.children(&mut cursor) {
        if let Some(found) = find_tag_in_node(&child, tag_name, source) {
            return Some(found);
        }
    }
    
    None
}

/// Convert tree-sitter Point to our Position struct
pub fn point_to_position(point: Point, offset: usize) -> crate::yaml::location::Position {
    crate::yaml::location::Position::new(
        point.row + 1,        // tree-sitter uses 0-based rows, we use 1-based
        point.column + 1,     // tree-sitter uses 0-based columns, we use 1-based
        offset,
    )
}

/// High-level function to find a tag position using a YAML path and tree-sitter
pub fn find_tag_position_with_tree_sitter(
    source: &str,
    yaml_path: &str,
    tag_name: &str,
) -> Result<crate::yaml::location::Position> {
    let mut parser = create_yaml_parser()?;
    let tree = parse_yaml_source(&mut parser, source)?;
    let root = tree.root_node();
    
    // Parse the YAML path (e.g., "Resources.MyBucket.Properties")
    let path_segments: Vec<&str> = if yaml_path.is_empty() {
        vec![]
    } else {
        yaml_path.split('.').collect()
    };
    
    // Handle array indices in path (e.g., "Resources.Items[2].operation")
    let (clean_path, index_positions) = parse_path_with_indices(&path_segments);
    
    
    // Handle simple case without array indices first
    if index_positions.is_empty() {
        // Simple path like "Resources.MyBucket.Properties.BucketName"
        let target_node = find_yaml_node_by_path(&root, &clean_path, source)
            .ok_or_else(|| anyhow!("Could not find YAML path: {}", yaml_path))?;
        
        // Find the tag within this node
        let tag_node = find_tag_in_node(&target_node, tag_name, source)
            .ok_or_else(|| anyhow!("Could not find tag {} in target node", tag_name))?;
        
        let start_point = tag_node.start_position();
        let start_offset = tag_node.start_byte();
        return Ok(point_to_position(start_point, start_offset));
    }
    
    // Complex case with array indices
    // For path like "Resources.Resource2.Properties.Value[0]", we:
    // 1. Navigate to "Resources.Resource2.Properties.Value" 
    // 2. Apply array index [0] to Value
    // 3. Continue with any remaining path segments
    
    let mut target_node = root;
    
    for (path_idx, &segment) in clean_path.iter().enumerate() {
        // Navigate to the next path segment
        if path_idx == 0 {
            // For the first segment, use full path navigation from root
            target_node = find_yaml_node_by_path(&target_node, &[segment], source)
                .ok_or_else(|| anyhow!("Could not find YAML path segment: {}", segment))?;
        } else {
            // For subsequent segments, we need to navigate from current position
            // First get to the mapping if we're at a mapping pair
            let mut search_node = target_node;
            if target_node.kind() == "block_mapping_pair" {
                if let Some(value_node) = target_node.child_by_field_name("value") {
                    search_node = value_node;
                }
            }
            
            // If search_node is a block_node, navigate to its mapping
            if search_node.kind() == "block_node" {
                let mut cursor = search_node.walk();
                for child in search_node.named_children(&mut cursor) {
                    if child.kind() == "block_mapping" {
                        search_node = child;
                        break;
                    }
                }
            }
            
            // Now find the child by key
            target_node = find_child_by_key(&search_node, segment, source)
                .ok_or_else(|| anyhow!("Could not find YAML path segment: {}", segment))?;
        }
        
        // Check if we need to apply an array index after this path segment
        if let Some(&(_, array_index)) = index_positions.iter().find(|(seg_idx, _)| *seg_idx == path_idx) {
            // We need to navigate into the value of this mapping pair to get to the sequence
            if target_node.kind() == "block_mapping_pair" {
                if let Some(value_node) = target_node.child_by_field_name("value") {
                    target_node = value_node;
                }
            }
            
            // If target_node is a block_node containing a block_sequence, navigate into it
            if target_node.kind() == "block_node" {
                let mut cursor = target_node.walk();
                for child in target_node.named_children(&mut cursor) {
                    if child.kind() == "block_sequence" {
                        target_node = child;
                        break;
                    }
                }
            }
            
            // Apply the array index
            target_node = find_array_element(&target_node, array_index, source)
                .ok_or_else(|| anyhow!("Could not find array index {} in path", array_index))?;
            
            // If we have more path segments, we need to navigate into the block_mapping of this array element
            if path_idx + 1 < clean_path.len() && target_node.kind() == "block_node" {
                let mut cursor = target_node.walk();
                for child in target_node.named_children(&mut cursor) {
                    if child.kind() == "block_mapping" {
                        target_node = child;
                        break;
                    }
                }
            }
        }
    }
    
    // Find the tag within this node
    let tag_node = find_tag_in_node(&target_node, tag_name, source)
        .ok_or_else(|| anyhow!("Could not find tag {} in target node", tag_name))?;
    
    // Convert to our position format
    let start_point = tag_node.start_position();
    let start_offset = tag_node.start_byte();
    
    Ok(point_to_position(start_point, start_offset))
}

/// Parse a path with array indices like ["Resources", "Items[2]", "operation"]
/// Returns (clean_path, index_positions) where clean_path has indices removed 
/// and index_positions maps segment index to array index
pub fn parse_path_with_indices<'a>(path_segments: &'a [&'a str]) -> (Vec<&'a str>, Vec<(usize, usize)>) {
    let mut clean_path = Vec::new();
    let mut index_positions = Vec::new();
    
    for (_segment_idx, &segment) in path_segments.iter().enumerate() {
        if let Some(bracket_start) = segment.find('[') {
            if let Some(bracket_end) = segment.find(']') {
                // Extract the key part before the bracket
                let key_part = &segment[..bracket_start];
                if !key_part.is_empty() {
                    clean_path.push(key_part);
                }
                
                // Extract and parse the index
                let index_str = &segment[bracket_start + 1..bracket_end];
                if let Ok(array_index) = index_str.parse::<usize>() {
                    // Record that after this clean_path segment, we need to apply this array index
                    index_positions.push((clean_path.len() - 1, array_index));
                }
            }
        } else {
            clean_path.push(segment);
        }
    }
    
    (clean_path, index_positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_yaml_parsing() {
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

        let result = find_tag_position_with_tree_sitter(yaml_source, "Resources.MyBucket.Properties.BucketName", "!$if");
        assert!(result.is_ok(), "Should find !$if tag: {:?}", result);
        
        let position = result.unwrap();
        assert_eq!(position.line, 6); // The line with !$if
        println!("Found !$if at line {}, column {}", position.line, position.column);
    }

    #[test]
    fn test_array_indexing() {
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

        // Test finding the third !$map (index 2)
        let result = find_tag_position_with_tree_sitter(yaml_source, "ListOperations[2].operation", "!$map");
        assert!(result.is_ok(), "Should find third !$map tag: {:?}", result);
        
        let position = result.unwrap();
        assert_eq!(position.line, 9); // The line with the third !$map
        println!("Found third !$map at line {}, column {}", position.line, position.column);
    }

    #[test]
    fn test_path_parsing_with_indices() {
        let path = vec!["Resources", "Items[2]", "operation"];
        let (clean_path, index_positions) = parse_path_with_indices(&path);
        
        assert_eq!(clean_path, vec!["Resources", "Items", "operation"]);
        assert_eq!(index_positions, vec![(1, 2)]); // index 2 applies after segment 1 ("Items")
    }
}