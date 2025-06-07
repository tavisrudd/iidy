/// Spike tests for the enhanced error reporting system
use crate::yaml::{EnhancedPreprocessingError, SourceLocation, ErrorId};

pub fn test_enhanced_error_display() {
    println!("=== Testing Enhanced Error Display ===\n");
    
    // Test 1: Variable not found error
    println!("--- Test 1: Variable Not Found ---");
    let location = SourceLocation::new("template.yaml", 4, 12, "<root>.result");
    let available_vars = vec!["app_name".to_string(), "environment".to_string(), "config".to_string()];
    
    let error = EnhancedPreprocessingError::variable_not_found(
        "app_nme", // typo in app_name
        location,
        available_vars,
    );
    
    let source_lines = vec![
        "# Configuration template".to_string(),
        "$defs:".to_string(),
        "  app_name: \"my-app\"".to_string(),
        "result: !$ app_nme".to_string(),
    ];
    
    println!("{}", error.display_with_context(Some(&source_lines)));
    
    // Test 2: Type mismatch error
    println!("--- Test 2: Type Mismatch ---");
    let location2 = SourceLocation::new("template.yaml", 6, 10, "<root>.map_result.items");
    let error2 = EnhancedPreprocessingError::type_mismatch(
        "array",
        "string", 
        location2,
        "!$map operation requires items to be an array",
    );
    
    let source_lines2 = vec![
        "$defs:".to_string(),
        "  not_array: \"I am a string\"".to_string(),
        "".to_string(),
        "map_result: !$map".to_string(),
        "  items: !$ not_array".to_string(),
        "  template: \"{{item}}\"".to_string(),
    ];
    
    println!("{}", error2.display_with_context(Some(&source_lines2)));
    
    // Test 3: Missing required field
    println!("--- Test 3: Missing Required Field ---");
    let location3 = SourceLocation::new("template.yaml", 3, 1, "<root>.broken_map");
    let error3 = EnhancedPreprocessingError::missing_required_field(
        "!$map",
        "template",
        location3,
        vec!["items".to_string(), "template".to_string()],
    );
    
    let source_lines3 = vec![
        "$defs:".to_string(),
        "  items: [1, 2, 3]".to_string(),
        "broken_map: !$map".to_string(),
        "  items: !$ items".to_string(),
        "  # missing template field".to_string(),
    ];
    
    println!("{}", error3.display_with_context(Some(&source_lines3)));
}

pub fn test_error_id_system() {
    println!("=== Testing Error ID System ===\n");
    
    // Test error code generation
    println!("--- Error Code Generation ---");
    println!("Variable not found: {}", ErrorId::VariableNotFound.code());
    println!("Type mismatch: {}", ErrorId::TypeMismatchInOperation.code());
    println!("Missing field: {}", ErrorId::MissingRequiredTagField.code());
    println!("Import error: {}", ErrorId::ImportFileNotFound.code());
    println!();
    
    // Test categories
    println!("--- Categories ---");
    println!("Variable errors: {}", ErrorId::VariableNotFound.category());
    println!("Type errors: {}", ErrorId::TypeMismatchInOperation.category());
    println!("Tag errors: {}", ErrorId::MissingRequiredTagField.category());
    println!("Import errors: {}", ErrorId::ImportFileNotFound.category());
    println!();
    
    // Test parsing error codes
    println!("--- Error Code Parsing ---");
    println!("Parse 'IY2001': {:?}", ErrorId::from_code("IY2001"));
    println!("Parse 'iy2001': {:?}", ErrorId::from_code("iy2001"));
    println!("Parse '2001': {:?}", ErrorId::from_code("2001"));
    println!("Parse '9999': {:?}", ErrorId::from_code("9999"));
    println!();
    
    // Test category filtering
    println!("--- Category Filtering ---");
    let variable_errors = ErrorId::in_category(2);
    println!("Variable category errors: {:?}", variable_errors.iter().map(|e| e.code()).collect::<Vec<_>>());
    
    let import_errors = ErrorId::in_category(3);
    println!("Import category errors: {:?}", import_errors.iter().map(|e| e.code()).collect::<Vec<_>>());
}

pub fn test_fuzzy_matching() {
    println!("=== Testing Fuzzy Matching ===\n");
    
    let available_vars = vec![
        "app_name".to_string(),
        "application_name".to_string(),
        "config".to_string(),
        "database_url".to_string(),
        "environment".to_string(),
        "user_config".to_string(),
    ];
    
    let test_cases = vec![
        "app_nme",      // Should suggest app_name
        "apliction",    // Should suggest application_name  
        "databse",      // Should suggest database_url
        "enviornment",  // Should suggest environment
        "xyz",          // Should suggest nothing close
    ];
    
    for typo in test_cases {
        let location = SourceLocation::new("test.yaml", 1, 1, "<root>");
        let error = EnhancedPreprocessingError::variable_not_found(
            typo,
            location,
            available_vars.clone(),
        );
        
        println!("Typo '{}' suggestions:", typo);
        if let EnhancedPreprocessingError::VariableNotFound { suggestions, .. } = error {
            for suggestion in suggestions {
                println!("  -> {}", suggestion);
            }
        }
        println!();
    }
}

pub fn test_realistic_scenarios() {
    println!("=== Testing Realistic Error Scenarios ===\n");
    
    // Scenario 1: CloudFormation template with variable typo
    println!("--- CloudFormation Template Error ---");
    let cf_source = vec![
        "AWSTemplateFormatVersion: '2010-09-09'".to_string(),
        "$defs:".to_string(),
        "  app_name: \"my-cloudformation-app\"".to_string(),
        "  environment: \"production\"".to_string(),
        "".to_string(),
        "Resources:".to_string(),
        "  MyBucket:".to_string(),
        "    Type: AWS::S3::Bucket".to_string(),
        "    Properties:".to_string(),
        "      BucketName: !$join".to_string(),
        "        array: [!$ app_nam, \"-\", !$ environment, \"-bucket\"]".to_string(),
        "        delimiter: \"\"".to_string(),
    ];
    
    let location = SourceLocation::new("cloudformation.yaml", 11, 20, "<root>.Resources.MyBucket.Properties.BucketName.array[0]");
    let available_vars = vec!["app_name".to_string(), "environment".to_string()];
    let error = EnhancedPreprocessingError::variable_not_found("app_nam", location, available_vars);
    
    println!("{}", error.display_with_context(Some(&cf_source)));
    
    // Scenario 2: Complex nested error in map operation
    println!("--- Nested Map Operation Error ---");
    let nested_source = vec![
        "$imports:".to_string(),
        "  services: file:services.yaml".to_string(),
        "".to_string(),
        "processed_services: !$map".to_string(),
        "  items: !$ services".to_string(),
        "  template: !$merge".to_string(),
        "    - name: \"{{item.service_nme}}\"".to_string(),
        "    - processed: true".to_string(),
    ];
    
    let location2 = SourceLocation::new("main.yaml", 7, 21, "<root>.processed_services.template[0].name");
    let available_vars = vec!["item".to_string(), "itemIdx".to_string()];
    let error2 = EnhancedPreprocessingError::variable_not_found("service_nme", location2, available_vars);
    
    println!("{}", error2.display_with_context(Some(&nested_source)));
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn spike_run_all_tests() {
        test_error_id_system();
        test_enhanced_error_display();
        test_fuzzy_matching();
        test_realistic_scenarios();
    }
}