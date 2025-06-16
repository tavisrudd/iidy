//! Debug test to see what YAML paths are being generated

use iidy::yaml::parsing::ParseContext;

#[test]
fn test_yaml_path_generation() {
    let source = r#"Resources:
  MapOp1: !$map
    items: [1, 2, 3]
    template: "{{item}}"
  
  MapOp2: !$map
    items: [a, b, c]
    template: "prefix-{{item}}"
  
  MapOp3: !$map
    items: [x, y, z]
    # missing template field"#;

    let context = ParseContext::new("test.yaml", source);

    // Test path building
    let ctx1 = context.with_path("Resources");
    println!("Path after Resources: '{}'", ctx1.yaml_path);

    let ctx2 = ctx1.with_path("MapOp1");
    println!("Path after MapOp1: '{}'", ctx2.yaml_path);

    let ctx3 = ctx2.with_path("items");
    println!("Path after items: '{}'", ctx3.yaml_path);

    let ctx4 = ctx1.with_path("MapOp3");
    println!("Path after MapOp3: '{}'", ctx4.yaml_path);

    // Test context-aware finding with different paths
    println!("\nTesting context-aware finding:");

    let mapop1_context = context.with_path("Resources").with_path("MapOp1");
    if let Some(pos) = mapop1_context.find_tag_position_in_context("!$map") {
        println!(
            "MapOp1 context finds !$map at line {}, column {}",
            pos.line, pos.column
        );
    }

    let mapop3_context = context.with_path("Resources").with_path("MapOp3");
    if let Some(pos) = mapop3_context.find_tag_position_in_context("!$map") {
        println!(
            "MapOp3 context finds !$map at line {}, column {}",
            pos.line, pos.column
        );
    }
}
