//! Tests specifically for handling multiple occurrences in position tracking

use iidy::yaml::parser::ParseContext;

#[test]
fn test_find_specific_occurrence_of_tag() {
    let yaml_content = r#"Resources:
  FirstTemplate: !$map
    items: [1, 2, 3]
    template: "{{item}}"
  
  SecondTemplate: !$map
    items: [a, b, c]
    template: !$if
      test: !$eq [1, 2]
      then: "yes"
      else: "no"
  
  ThirdTemplate: !$map
    items: [x, y, z]
    template: value"#;

    let context = ParseContext::new("test.yaml", yaml_content);
    
    // Find first occurrence of !$map
    let first_map = context.find_position_of("!$map").unwrap();
    assert_eq!(first_map.line, 2);
    assert_eq!(first_map.column, 18);
    
    // Find second occurrence of !$map using offset
    let first_end = first_map.offset + "!$map".len();
    let second_map = context.find_position_of_from_offset("!$map", first_end).unwrap();
    assert_eq!(second_map.line, 6);
    assert_eq!(second_map.column, 19);
    
    // Find third occurrence of !$map using offset  
    let second_end = second_map.offset + "!$map".len();
    let third_map = context.find_position_of_from_offset("!$map", second_end).unwrap();
    assert_eq!(third_map.line, 13);
    assert_eq!(third_map.column, 18);
    
    // Verify there's no fourth occurrence
    let third_end = third_map.offset + "!$map".len();
    let fourth_map = context.find_position_of_from_offset("!$map", third_end);
    assert!(fourth_map.is_none());
}

#[test]
fn test_find_nested_tags_with_same_name() {
    let yaml_content = r#"Nested:
  Level1: !$if
    test: true
    then: !$if  
      test: false
      then: "deep"
      else: !$if
        test: !$eq [a, b]
        then: "deeper"
        else: "deepest""#;

    let context = ParseContext::new("nested.yaml", yaml_content);
    
    // Find all three !$if tags
    let first_if = context.find_position_of("!$if").unwrap();
    assert_eq!(first_if.line, 2);
    assert_eq!(first_if.column, 11);
    
    let first_end = first_if.offset + "!$if".len();
    let second_if = context.find_position_of_from_offset("!$if", first_end).unwrap();
    assert_eq!(second_if.line, 4);
    assert_eq!(second_if.column, 11);
    
    let second_end = second_if.offset + "!$if".len();
    let third_if = context.find_position_of_from_offset("!$if", second_end).unwrap();
    assert_eq!(third_if.line, 7);
    assert_eq!(third_if.column, 13);
}

#[test]
fn test_find_template_keyword_multiple_times() {
    let yaml_content = r#"MapOperations:
  First: !$map
    items: [1, 2]
    template: "{{item}}"
  
  Second: !$concatMap
    items: [a, b]
    template: "prefix-{{item}}"
  
  Third: !$mergeMap
    items: [x, y]
    template: 
      key: "{{item}}"
      value: 123"#;

    let context = ParseContext::new("templates.yaml", yaml_content);
    
    // Find all occurrences of "template:"
    let first_template = context.find_position_of("template:").unwrap();
    assert_eq!(first_template.line, 4);
    assert_eq!(first_template.column, 5);
    
    let first_end = first_template.offset + "template:".len();
    let second_template = context.find_position_of_from_offset("template:", first_end).unwrap();
    assert_eq!(second_template.line, 8);
    assert_eq!(second_template.column, 5);
    
    let second_end = second_template.offset + "template:".len();
    let third_template = context.find_position_of_from_offset("template:", second_end).unwrap();
    assert_eq!(third_template.line, 12);
    assert_eq!(third_template.column, 5);
}

#[test]
fn test_find_items_keyword_multiple_times() {
    let yaml_content = r#"Configuration:
  ListOps:
    - items: [1, 2, 3]
      template: "num-{{item}}"
    - items: [a, b, c] 
      template: "str-{{item}}"
  
  MapOps:
    operation1:
      items: [x, y, z]
      var: element
    operation2:
      items: [p, q, r]
      filter: "{{item}} != 'q'""#;

    let context = ParseContext::new("items.yaml", yaml_content);
    
    // Find all occurrences of "items:"
    let occurrences: Vec<_> = (0..6)
        .scan(0, |offset, _| {
            let result = context.find_position_of_from_offset("items:", *offset);
            if let Some(pos) = &result {
                *offset = pos.offset + "items:".len();
            }
            Some(result)
        })
        .take_while(|opt| opt.is_some())
        .map(|opt| opt.unwrap())
        .collect();
    
    assert_eq!(occurrences.len(), 4);
    
    // Verify positions
    assert_eq!(occurrences[0].line, 3);
    assert_eq!(occurrences[0].column, 7);
    
    assert_eq!(occurrences[1].line, 5);
    assert_eq!(occurrences[1].column, 7);
    
    assert_eq!(occurrences[2].line, 10);
    assert_eq!(occurrences[2].column, 7);
    
    assert_eq!(occurrences[3].line, 13);
    assert_eq!(occurrences[3].column, 7);
}

#[test]
fn test_find_common_values_across_lines() {
    let yaml_content = r#"CommonValues:
  test: value
  other: test
  nested:
    test: another
    value: test
  array:
    - test
    - value  
    - test"#;

    let context = ParseContext::new("common.yaml", yaml_content);
    
    // Find all occurrences of "test"
    let mut test_positions = Vec::new();
    let mut offset = 0;
    while let Some(pos) = context.find_position_of_from_offset("test", offset) {
        offset = pos.offset + "test".len();
        test_positions.push(pos);
    }
    
    // Should find 5 occurrences: test:, other: test, test:, value: test, - test, - test
    assert_eq!(test_positions.len(), 6);
    
    // Verify some key positions
    assert_eq!(test_positions[0].line, 2); // test: value
    assert_eq!(test_positions[0].column, 3);
    
    assert_eq!(test_positions[1].line, 3); // other: test
    assert_eq!(test_positions[1].column, 10);
    
    assert_eq!(test_positions[2].line, 5); // nested test:
    assert_eq!(test_positions[2].column, 5);
    
    assert_eq!(test_positions[3].line, 6); // value: test
    assert_eq!(test_positions[3].column, 12);
    
    assert_eq!(test_positions[4].line, 8); // - test
    assert_eq!(test_positions[4].column, 7);
    
    assert_eq!(test_positions[5].line, 10); // - test
    assert_eq!(test_positions[5].column, 7);
}

#[test]
fn test_position_tracking_validates_issue_is_fixed() {
    // This test specifically validates that the issue mentioned in the original question is fixed
    // The issue was that find_position_of always searches from the beginning
    
    let source = "tag: first\nsame_tag: second\nanother_tag: third";
    let context = ParseContext::new("test.yaml", source);
    
    // The old behavior (which was incorrect) would always find the first occurrence
    let first_tag = context.find_position_of("tag").unwrap();
    assert_eq!(first_tag.line, 1);
    assert_eq!(first_tag.column, 1);
    assert_eq!(first_tag.offset, 0);
    
    // With the corrected find_position_of_from_offset, we can find subsequent occurrences
    let second_tag = context.find_position_of_from_offset("tag", first_tag.offset + 1).unwrap();
    assert_eq!(second_tag.line, 2);
    assert_eq!(second_tag.column, 6); // "same_" = 5 chars, so "tag" starts at column 6
    
    let third_tag = context.find_position_of_from_offset("tag", second_tag.offset + 1).unwrap();
    assert_eq!(third_tag.line, 3);
    assert_eq!(third_tag.column, 9); // "another_" = 8 chars, so "tag" starts at column 9
    
    // Verify no more occurrences
    let no_more = context.find_position_of_from_offset("tag", third_tag.offset + 1);
    assert!(no_more.is_none());
}

#[test]
fn test_offset_boundary_edge_cases() {
    let source = "abc def abc ghi abc";
    let context = ParseContext::new("boundary.yaml", source);
    
    // Find first "abc"
    let first = context.find_position_of_from_offset("abc", 0).unwrap();
    assert_eq!(first.offset, 0);
    
    // Start search right after first "abc"
    let second = context.find_position_of_from_offset("abc", first.offset + "abc".len()).unwrap();
    assert_eq!(second.offset, 8); // "abc def " = 8 chars
    
    // Start search right after second "abc"
    let third = context.find_position_of_from_offset("abc", second.offset + "abc".len()).unwrap();
    assert_eq!(third.offset, 16); // "abc def abc ghi " = 16 chars
    
    // Start search right after third "abc" - should find nothing
    let none = context.find_position_of_from_offset("abc", third.offset + "abc".len());
    assert!(none.is_none());
}