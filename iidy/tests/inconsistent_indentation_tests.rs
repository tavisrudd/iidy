//! Tests for inconsistent indentation handling in ParseContext

use iidy::yaml::parser::{ParseContext, ParseConfig};

#[test]
fn test_mixed_tabs_and_spaces() {
    let yaml_content = r#"Resources:
  FirstLevel: !$if
    test: true
    then: "spaces"
	SecondLevel: !$if
		test: false
		then: "tabs"
  ThirdLevel: !$if
    test: true
    then: "mixed""#;

    let context = ParseContext::new("mixed.yaml", yaml_content);
    
    // Test that we can still find the correct tag even with mixed indentation
    let first_ctx = context.with_path("Resources").with_path("FirstLevel");
    let second_ctx = context.with_path("Resources").with_path("SecondLevel");
    let third_ctx = context.with_path("Resources").with_path("ThirdLevel");
    
    // All should find their respective !$if tags
    assert!(first_ctx.find_tag_position_in_context("!$if").is_some());
    assert!(second_ctx.find_tag_position_in_context("!$if").is_some());
    assert!(third_ctx.find_tag_position_in_context("!$if").is_some());
    
    // Verify line numbers are correct
    let first_pos = first_ctx.find_tag_position_in_context("!$if").unwrap();
    let second_pos = second_ctx.find_tag_position_in_context("!$if").unwrap();
    let third_pos = third_ctx.find_tag_position_in_context("!$if").unwrap();
    
    assert_eq!(first_pos.line, 2);
    assert_eq!(second_pos.line, 5);
    assert_eq!(third_pos.line, 8);
}

#[test]
fn test_varying_indent_sizes() {
    let yaml_content = r#"Resources:
 Tag1: !$map  # 1 space
   items: [1, 2]
   template: "{{item}}"
  Tag2: !$map  # 2 spaces
    items: [3, 4]
    template: "{{item}}"
    Tag3: !$map  # 4 spaces (nested deeper)
        items: [5, 6]
        template: "{{item}}"
     Tag4: !$map  # 5 spaces (inconsistent)
         items: [7, 8]
         template: "{{item}}""#;

    let context = ParseContext::new("varying.yaml", yaml_content);
    
    // Test that we can find each tag correctly despite varying indentation
    let tag1_ctx = context.with_path("Resources").with_path("Tag1");
    let tag2_ctx = context.with_path("Resources").with_path("Tag2");
    let tag3_ctx = context.with_path("Resources").with_path("Tag3");
    let tag4_ctx = context.with_path("Resources").with_path("Tag4");
    
    let tag1_pos = tag1_ctx.find_tag_position_in_context("!$map").unwrap();
    let tag2_pos = tag2_ctx.find_tag_position_in_context("!$map").unwrap();
    let tag3_pos = tag3_ctx.find_tag_position_in_context("!$map").unwrap();
    let tag4_pos = tag4_ctx.find_tag_position_in_context("!$map").unwrap();
    
    assert_eq!(tag1_pos.line, 2);
    assert_eq!(tag2_pos.line, 5);
    assert_eq!(tag3_pos.line, 8);
    assert_eq!(tag4_pos.line, 11);
}

#[test]
fn test_auto_detect_indent_size() {
    // Test 4-space indentation
    let yaml_4_spaces = r#"Resources:
    Level1:
        Level2:
            Level3: value"#;
    
    let config_4 = ParseConfig::auto_detect_indent(yaml_4_spaces);
    assert_eq!(config_4.indent_size, 4);
    
    // Test 2-space indentation
    let yaml_2_spaces = r#"Resources:
  Level1:
    Level2:
      Level3: value"#;
    
    let config_2 = ParseConfig::auto_detect_indent(yaml_2_spaces);
    assert_eq!(config_2.indent_size, 2);
    
    // Test inconsistent indentation (should default to 2)
    let yaml_mixed = r#"Resources:
 Level1:
   Level2:
     Level3: value"#;
    
    let config_mixed = ParseConfig::auto_detect_indent(yaml_mixed);
    assert_eq!(config_mixed.indent_size, 2); // Should fall back to default
}

#[test]
fn test_indentation_with_custom_config() {
    let yaml_content = r#"Resources:
  Tag1: !$if
    test: true
    then: "first"
      Tag2: !$if
        test: false
        then: "second"
        Tag3: !$if
          test: true
          then: "third""#;

    // Use custom indent size of 2
    let config = ParseConfig::with_indent_size(2);
    let context = ParseContext::with_config("test.yaml", yaml_content, config);
    
    // Test that depth estimation works with custom config
    let tag1_ctx = context.with_path("Resources").with_path("Tag1");
    let tag2_ctx = context.with_path("Resources").with_path("Tag1").with_path("Tag2");
    let tag3_ctx = context.with_path("Resources").with_path("Tag1").with_path("Tag2").with_path("Tag3");
    
    // All should find their respective tags
    assert!(tag1_ctx.find_tag_position_in_context("!$if").is_some());
    assert!(tag2_ctx.find_tag_position_in_context("!$if").is_some());
    assert!(tag3_ctx.find_tag_position_in_context("!$if").is_some());
}

#[test]
fn test_tab_based_indentation() {
    let yaml_content = "Resources:\n\tTag1: !$if\n\t\ttest: true\n\t\tthen: \"tab_based\"\n\t\tTag2: !$if\n\t\t\ttest: false\n\t\t\tthen: \"nested_tabs\"";

    let context = ParseContext::new("tabs.yaml", yaml_content);
    
    let tag1_ctx = context.with_path("Resources").with_path("Tag1");
    let tag2_ctx = context.with_path("Resources").with_path("Tag1").with_path("Tag2");
    
    let tag1_pos = tag1_ctx.find_tag_position_in_context("!$if").unwrap();
    let tag2_pos = tag2_ctx.find_tag_position_in_context("!$if").unwrap();
    
    assert_eq!(tag1_pos.line, 2);
    assert_eq!(tag2_pos.line, 5);
}

#[test]
fn test_inconsistent_mixed_indentation_real_world() {
    // Real-world example with inconsistent indentation that might confuse parsers
    let yaml_content = r#"Resources:
 Database: !$if
   test: !$eq ["{{env}}", "prod"]
   then:
     Type: "AWS::RDS::DBInstance"
     Properties:
	Engine: "postgres"  # Mixed tab here
	InstanceClass: "db.t3.micro"
   else:
      Type: "AWS::RDS::DBInstance"  # Different indent here
      Properties:
        Engine: "postgres"
        InstanceClass: "db.t3.nano"
 WebServer: !$if  # Back to inconsistent spacing
    test: !$not [!$eq ["{{env}}", "test"]]
    then:
      Type: "AWS::EC2::Instance"
      Properties:
        InstanceType: "t3.medium""#;

    let context = ParseContext::new("inconsistent.yaml", yaml_content);
    
    // Test finding the first !$if (Database)
    let db_ctx = context.with_path("Resources").with_path("Database");
    let db_pos = db_ctx.find_tag_position_in_context("!$if").unwrap();
    assert_eq!(db_pos.line, 2);
    
    // Test finding the !$eq inside the Database !$if
    let db_test_ctx = db_ctx.with_path("test");
    let eq_pos = db_test_ctx.find_tag_position_in_context("!$eq").unwrap();
    assert_eq!(eq_pos.line, 3);
    
    // Test finding the second !$if (WebServer)
    let web_ctx = context.with_path("Resources").with_path("WebServer");
    let web_pos = web_ctx.find_tag_position_in_context("!$if").unwrap();
    assert_eq!(web_pos.line, 14);
    
    // Test finding nested tags with complex indentation
    let web_test_ctx = web_ctx.with_path("test");
    let not_pos = web_test_ctx.find_tag_position_in_context("!$not").unwrap();
    assert_eq!(not_pos.line, 15);
}

#[test]
fn test_empty_lines_and_comments_with_inconsistent_indentation() {
    let yaml_content = r#"Resources:
  # First section with normal indentation
  Tag1: !$map
    items: [1, 2, 3]
    template: "item_{{item}}"

    # Comment with weird spacing

     # Another comment

  Tag2: !$if  # Inconsistent indent after empty lines
     test: true
     then: "success"

# Top level comment

	Tag3: !$merge  # Tab indented after comment
		- source1
		- source2"#;

    let context = ParseContext::new("comments.yaml", yaml_content);
    
    let tag1_ctx = context.with_path("Resources").with_path("Tag1");
    let tag2_ctx = context.with_path("Resources").with_path("Tag2");
    let tag3_ctx = context.with_path("Resources").with_path("Tag3");
    
    let tag1_pos = tag1_ctx.find_tag_position_in_context("!$map").unwrap();
    let tag2_pos = tag2_ctx.find_tag_position_in_context("!$if").unwrap();
    let tag3_pos = tag3_ctx.find_tag_position_in_context("!$merge").unwrap();
    
    assert_eq!(tag1_pos.line, 3);
    assert_eq!(tag2_pos.line, 11);
    assert_eq!(tag3_pos.line, 17);
}