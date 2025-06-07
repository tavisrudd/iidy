//! Test array index extraction from YAML paths

use iidy::yaml::parser::ParseContext;

#[test]
fn test_array_index_extraction() {
    let context = ParseContext::new("test.yaml", "content");
    
    // Test with no array index
    let ctx1 = context.with_path("Resources").with_path("MyBucket");
    println!("Path '{}' -> array index: {:?}", ctx1.yaml_path, ctx1.extract_array_index_from_path());
    assert_eq!(ctx1.extract_array_index_from_path(), None);
    
    // Test with array index at the end
    let ctx2 = context.with_path("Resources").with_array_index(5);
    println!("Path '{}' -> array index: {:?}", ctx2.yaml_path, ctx2.extract_array_index_from_path());
    assert_eq!(ctx2.extract_array_index_from_path(), Some(5));
    
    // Test with array index in the middle
    let ctx3 = context.with_path("Resources").with_array_index(3).with_path("operation");
    println!("Path '{}' -> array index: {:?}", ctx3.yaml_path, ctx3.extract_array_index_from_path());
    assert_eq!(ctx3.extract_array_index_from_path(), Some(3));
    
    // Test with multiple array indices (should get the first one)
    let ctx4 = context.with_path("Resources").with_array_index(2).with_path("nested").with_array_index(7);
    println!("Path '{}' -> array index: {:?}", ctx4.yaml_path, ctx4.extract_array_index_from_path());
    assert_eq!(ctx4.extract_array_index_from_path(), Some(2));
    
    // Test with complex path
    let ctx5 = context.with_path("ListOperations").with_array_index(12).with_path("operation").with_path("items");
    println!("Path '{}' -> array index: {:?}", ctx5.yaml_path, ctx5.extract_array_index_from_path());
    assert_eq!(ctx5.extract_array_index_from_path(), Some(12));
}