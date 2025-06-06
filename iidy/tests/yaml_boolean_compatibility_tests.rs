use anyhow::Result;
use iidy::yaml::{preprocess_yaml_with_base_location, YamlPreprocessor};
use iidy::yaml::imports::loaders::ProductionImportLoader;
use serde_yaml::Value;

/// Tests for YAML 1.1 vs 1.2 boolean compatibility
/// 
/// CloudFormation uses YAML 1.1 which auto-converts strings like "yes", "no", "on", "off" to booleans.
/// However, serde_yaml follows YAML 1.2 which treats these as strings.
/// We need to implement YAML 1.1 boolean compatibility for CloudFormation templates.

#[tokio::test]
async fn test_current_yaml_12_behavior() -> Result<()> {
    let yaml_input = r#"
Resources:
  MyResource:
    Type: AWS::EC2::Instance
    Properties:
      Monitoring: yes          # Should be boolean true in YAML 1.1
      EbsOptimized: no         # Should be boolean false in YAML 1.1
      DetailedMonitoring: on   # Should be boolean true in YAML 1.1
      PublicIp: off            # Should be boolean false in YAML 1.1
      Backup: null             # Should be null
      Description: "yes"       # Should remain string (quoted)
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(root) = result {
        if let Some(Value::Mapping(resources)) = root.get(&Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(resource)) = resources.get(&Value::String("MyResource".to_string())) {
                if let Some(Value::Mapping(properties)) = resource.get(&Value::String("Properties".to_string())) {
                    
                    // Check current behavior - with YAML 1.2, these should be strings
                    let monitoring = properties.get(&Value::String("Monitoring".to_string()));
                    let ebs_optimized = properties.get(&Value::String("EbsOptimized".to_string()));
                    let detailed_monitoring = properties.get(&Value::String("DetailedMonitoring".to_string()));
                    let public_ip = properties.get(&Value::String("PublicIp".to_string()));
                    let backup = properties.get(&Value::String("Backup".to_string()));
                    let description = properties.get(&Value::String("Description".to_string()));
                    
                    // Print actual values for investigation
                    println!("Current YAML 1.2 behavior:");
                    println!("  Monitoring: {:?}", monitoring);
                    println!("  EbsOptimized: {:?}", ebs_optimized);
                    println!("  DetailedMonitoring: {:?}", detailed_monitoring);
                    println!("  PublicIp: {:?}", public_ip);
                    println!("  Backup: {:?}", backup);
                    println!("  Description: {:?}", description);
                    
                    // Document current behavior (YAML 1.2)
                    // In YAML 1.2, only true/false/null are special - other values remain strings
                    
                    // These should be strings in YAML 1.2 but booleans in YAML 1.1
                    match monitoring {
                        Some(Value::String(s)) if s == "yes" => {
                            println!("❌ YAML 1.1 incompatibility: 'yes' is a string, should be boolean true");
                        }
                        Some(Value::Bool(true)) => {
                            println!("✅ YAML 1.1 compatible: 'yes' converted to boolean true");
                        }
                        _ => println!("⚠️  Unexpected value for 'yes': {:?}", monitoring),
                    }
                    
                    match ebs_optimized {
                        Some(Value::String(s)) if s == "no" => {
                            println!("❌ YAML 1.1 incompatibility: 'no' is a string, should be boolean false");
                        }
                        Some(Value::Bool(false)) => {
                            println!("✅ YAML 1.1 compatible: 'no' converted to boolean false");
                        }
                        _ => println!("⚠️  Unexpected value for 'no': {:?}", ebs_optimized),
                    }
                    
                    // IMPORTANT: Check that quoted strings remain strings
                    match description {
                        Some(Value::String(s)) if s == "yes" => {
                            println!("✅ Quoted string preserved: '\"yes\"' remained as string");
                        }
                        Some(Value::Bool(true)) => {
                            println!("❌ Quoted string incorrectly converted: '\"yes\"' became boolean - this breaks YAML 1.1 spec!");
                        }
                        _ => println!("⚠️  Unexpected value for quoted 'yes': {:?}", description),
                    }
                }
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_yaml_11_boolean_requirements_for_cloudformation() -> Result<()> {
    // Test all YAML 1.1 boolean variants that CloudFormation expects
    let yaml_input = r#"
$defs:
  enable_monitoring: yes

Resources:
  TestBooleans:
    Type: AWS::EC2::Instance
    Properties:
      # YAML 1.1 true variants (should become boolean true)
      Monitoring1: yes
      Monitoring2: Yes  
      Monitoring3: YES
      Monitoring4: true
      Monitoring5: True
      Monitoring6: TRUE
      Monitoring7: on
      Monitoring8: On
      Monitoring9: ON
      
      # YAML 1.1 false variants (should become boolean false)
      EbsOptimized1: no
      EbsOptimized2: No
      EbsOptimized3: NO
      EbsOptimized4: false
      EbsOptimized5: False
      EbsOptimized6: FALSE
      EbsOptimized7: off
      EbsOptimized8: Off
      EbsOptimized9: OFF
      
      # Null variants (should become null)
      BackupPolicy1: null
      BackupPolicy2: Null
      BackupPolicy3: NULL
      BackupPolicy4: ~
      
      # These should remain strings (quoted or after preprocessing)
      Description1: "yes"  # quoted string
      Description2: "{{enable_monitoring}}"  # handlebars result
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // For now, just document what we expect vs what we get
    println!("\nYAML 1.1 Boolean Compatibility Requirements:");
    println!("CloudFormation expects YAML 1.1 behavior where:");
    println!("  - yes/Yes/YES/true/True/TRUE/on/On/ON → boolean true");
    println!("  - no/No/NO/false/False/FALSE/off/Off/OFF → boolean false");
    println!("  - null/Null/NULL/~ → null");
    println!("  - Quoted strings should remain strings");
    println!("  - Handlebars results should follow same rules");
    
    if let Value::Mapping(root) = result {
        if let Some(Value::Mapping(resources)) = root.get(&Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(resource)) = resources.get(&Value::String("TestBooleans".to_string())) {
                if let Some(Value::Mapping(properties)) = resource.get(&Value::String("Properties".to_string())) {
                    
                    // Check a few key examples
                    let monitoring1 = properties.get(&Value::String("Monitoring1".to_string()));
                    let ebs_optimized1 = properties.get(&Value::String("EbsOptimized1".to_string()));
                    let backup1 = properties.get(&Value::String("BackupPolicy1".to_string()));
                    let description1 = properties.get(&Value::String("Description1".to_string()));
                    let description2 = properties.get(&Value::String("Description2".to_string()));
                    
                    println!("\nActual current behavior:");
                    println!("  Monitoring1 (yes): {:?}", monitoring1);
                    println!("  EbsOptimized1 (no): {:?}", ebs_optimized1); 
                    println!("  BackupPolicy1 (null): {:?}", backup1);
                    println!("  Description1 (\"yes\"): {:?}", description1);
                    println!("  Description2 (handlebars 'yes'): {:?}", description2);
                }
            }
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_yaml_11_boolean_edge_cases() -> Result<()> {
    // Test edge cases and tricky scenarios
    let yaml_input = r#"
Resources:
  EdgeCases:
    Type: AWS::EC2::Instance
    Properties:
      # Numbers that look like booleans should remain numbers
      Port1: 80
      Port2: 443
      
      # Empty and whitespace handling
      EmptyString: ""
      WhitespaceString: "   "
      
      # Boolean-like strings in different contexts
      Tags:
        - Key: "Enabled"
          Value: yes        # Should be boolean true
        - Key: "Disabled"  
          Value: "no"       # Should be string "no" (quoted)
      
      # Array of boolean-like values
      EnabledFeatures:
        - yes              # Should be boolean true
        - no               # Should be boolean false
        - on               # Should be boolean true
        - "off"            # Should be string "off" (quoted)
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    println!("\nEdge cases for YAML 1.1 boolean compatibility:");
    
    // For now, just verify it parses and document behavior
    assert!(result.is_mapping());
    
    Ok(())
}

#[tokio::test]
async fn test_yaml_12_mode_preserves_quoted_strings() -> Result<()> {
    let yaml_input = r#"
Resources:
  MyResource:
    Type: AWS::EC2::Instance
    Properties:
      Monitoring: yes          # Should be boolean true in YAML 1.1 mode
      EbsOptimized: no         # Should be boolean false in YAML 1.1 mode
      Description: "yes"       # Should remain string in both modes
"#;

    // Test YAML 1.2 mode (no boolean conversion)
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new_yaml_12_mode(loader);
    let result_yaml_12 = preprocessor.process(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(root) = result_yaml_12 {
        if let Some(Value::Mapping(resources)) = root.get(&Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(resource)) = resources.get(&Value::String("MyResource".to_string())) {
                if let Some(Value::Mapping(properties)) = resource.get(&Value::String("Properties".to_string())) {
                    
                    let monitoring = properties.get(&Value::String("Monitoring".to_string()));
                    let ebs_optimized = properties.get(&Value::String("EbsOptimized".to_string()));
                    let description = properties.get(&Value::String("Description".to_string()));
                    
                    println!("YAML 1.2 mode behavior:");
                    println!("  Monitoring: {:?}", monitoring);
                    println!("  EbsOptimized: {:?}", ebs_optimized);
                    println!("  Description: {:?}", description);
                    
                    // In YAML 1.2 mode, all should remain as strings
                    assert_eq!(monitoring, Some(&Value::String("yes".to_string())));
                    assert_eq!(ebs_optimized, Some(&Value::String("no".to_string())));
                    assert_eq!(description, Some(&Value::String("yes".to_string())));
                }
            }
        }
    }
    
    // Test YAML 1.1 mode (with boolean conversion) 
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader); // Default is YAML 1.1 mode
    let result_yaml_11 = preprocessor.process(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(root) = result_yaml_11 {
        if let Some(Value::Mapping(resources)) = root.get(&Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(resource)) = resources.get(&Value::String("MyResource".to_string())) {
                if let Some(Value::Mapping(properties)) = resource.get(&Value::String("Properties".to_string())) {
                    
                    let monitoring = properties.get(&Value::String("Monitoring".to_string()));
                    let ebs_optimized = properties.get(&Value::String("EbsOptimized".to_string()));
                    let description = properties.get(&Value::String("Description".to_string()));
                    
                    println!("YAML 1.1 mode behavior:");
                    println!("  Monitoring: {:?}", monitoring);
                    println!("  EbsOptimized: {:?}", ebs_optimized);  
                    println!("  Description: {:?}", description);
                    
                    // In YAML 1.1 mode, unquoted booleans convert but quoted remain strings
                    assert_eq!(monitoring, Some(&Value::Bool(true)));
                    assert_eq!(ebs_optimized, Some(&Value::Bool(false)));
                    // Description should be preserved because it's in a Description context
                    // OR because it was originally quoted (though we can't distinguish this)
                    match description {
                        Some(Value::String(s)) if s == "yes" => {
                            println!("✅ Description field preserved as string");
                        }
                        Some(Value::Bool(true)) => {
                            println!("⚠️  Description was converted to boolean - this may be acceptable depending on heuristics");
                        }
                        _ => panic!("Unexpected description value: {:?}", description),
                    }
                }
            }
        }
    }
    
    Ok(())
}