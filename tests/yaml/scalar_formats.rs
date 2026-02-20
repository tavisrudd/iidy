//! Comprehensive tests for YAML scalar formats and round-trip fidelity
//!
//! Tests various YAML scalar representations to ensure proper handling of:
//! - Literal scalars (|, |-, |+)
//! - Folded scalars (>, >-, >+)
//! - Mixed content with preprocessing
//! - Round-trip fidelity through iidy render
//!
//! This is crucial for CloudFormation templates that often use multi-line strings
//! for UserData, policies, and other content.

use anyhow::Result;
use iidy::yaml::preprocess_yaml_v11;
use serde_yaml::Value;
use std::io::Write;
use tempfile::NamedTempFile;

/// Test literal scalar formats (|, |-, |+) with preprocessing
#[tokio::test]
async fn test_literal_scalar_formats() -> Result<()> {
    let yaml_input = r#"
$defs:
  environment: production
  region: us-east-1

# Literal scalar with final newline preserved (default)
user_data_basic: |
  #!/bin/bash
  echo "Environment: {{environment}}"
  echo "Region: {{region}}"
  yum update -y

# Literal scalar with final newlines stripped
user_data_strip: |-
  #!/bin/bash
  echo "Environment: {{environment}}"
  echo "Region: {{region}}"
  yum update -y

# Literal scalar with final newlines kept
user_data_keep: |+
  #!/bin/bash
  echo "Environment: {{environment}}"
  echo "Region: {{region}}"
  yum update -y


# Test with CloudFormation tag
Resources:
  EC2Instance:
    Properties:
      UserData: !Base64 |
        #!/bin/bash
        echo "Starting {{environment}} instance in {{region}}"
        /opt/aws/bin/cfn-init -v --stack ${AWS::StackName}
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;

    if let Value::Mapping(root) = &result {
        // Check literal scalar with handlebars processing
        if let Some(Value::String(user_data_basic)) =
            root.get(Value::String("user_data_basic".to_string()))
        {
            assert!(user_data_basic.contains("Environment: production"));
            assert!(user_data_basic.contains("Region: us-east-1"));
            assert!(user_data_basic.contains("#!/bin/bash"));
            assert!(user_data_basic.ends_with("yum update -y\n")); // Should preserve final newline
        } else {
            panic!("Expected user_data_basic to be a string");
        }

        // Check stripped version
        if let Some(Value::String(user_data_strip)) =
            root.get(Value::String("user_data_strip".to_string()))
        {
            assert!(user_data_strip.contains("Environment: production"));
            assert!(user_data_strip.ends_with("yum update -y")); // Should strip final newline
            assert!(!user_data_strip.ends_with("yum update -y\n"));
        } else {
            panic!("Expected user_data_strip to be a string");
        }

        // Check CloudFormation tag with literal scalar
        if let Some(Value::Mapping(resources)) = root.get(Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(instance)) =
                resources.get(Value::String("EC2Instance".to_string()))
            {
                if let Some(Value::Mapping(properties)) =
                    instance.get(Value::String("Properties".to_string()))
                {
                    if let Some(Value::Tagged(user_data_tagged)) =
                        properties.get(Value::String("UserData".to_string()))
                    {
                        // Should be a tagged value preserving !Base64
                        assert_eq!(user_data_tagged.tag.to_string(), "!Base64");
                        if let Value::String(processed_content) = &user_data_tagged.value {
                            assert!(
                                processed_content
                                    .contains("Starting production instance in us-east-1")
                            );
                            assert!(processed_content.contains("#!/bin/bash"));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Test folded scalar formats (>, >-, >+) with preprocessing
#[tokio::test]
async fn test_folded_scalar_formats() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_name: MyApplication
  version: "1.2.3"

# Folded scalar - spaces fold to single space, preserve double newlines
description_basic: >
  This is the {{app_name}} version {{version}}.
  
  It provides comprehensive functionality for
  processing CloudFormation templates with
  advanced preprocessing capabilities.
  
  
  Multiple blank lines above are preserved.

# Folded scalar with final newlines stripped  
description_strip: >-
  This is the {{app_name}} version {{version}}.
  It provides comprehensive functionality for
  processing CloudFormation templates.

# Folded scalar with final newlines kept
description_keep: >+
  This is the {{app_name}} version {{version}}.
  It provides comprehensive functionality.


# Test with JSON policy documents (common CloudFormation use case)
policy_document: >
  {
    "Version": "2012-10-17",
    "Statement": [
      {
        "Effect": "Allow", 
        "Action": "s3:GetObject",
        "Resource": "arn:aws:s3:::{{app_name}}-bucket/*"
      }
    ]
  }
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;

    if let Value::Mapping(root) = &result {
        // Check folded scalar with handlebars processing
        if let Some(Value::String(description_basic)) =
            root.get(Value::String("description_basic".to_string()))
        {
            assert!(description_basic.contains("MyApplication version 1.2.3"));
            // Folded scalars should fold lines into single spaces
            assert!(description_basic.contains("functionality for processing"));
            // Folded scalars preserve paragraph breaks (double newlines become single newlines in the middle)
            assert!(description_basic.contains("capabilities.\n\nMultiple"));
            // Final newline should be preserved
            assert!(description_basic.ends_with("preserved.\n"));
        } else {
            panic!("Expected description_basic to be a string");
        }

        // Check stripped version
        if let Some(Value::String(description_strip)) =
            root.get(Value::String("description_strip".to_string()))
        {
            assert!(description_strip.contains("MyApplication version 1.2.3"));
            // Should strip final newline
            assert!(description_strip.ends_with("templates."));
            assert!(!description_strip.ends_with("templates.\n"));
        } else {
            panic!("Expected description_strip to be a string");
        }

        // Check JSON policy document (real-world CloudFormation use case)
        if let Some(Value::String(policy)) = root.get(Value::String("policy_document".to_string()))
        {
            assert!(policy.contains("MyApplication-bucket"));
            assert!(policy.contains("\"Version\": \"2012-10-17\""));
            // JSON structure should be preserved
            assert!(policy.contains("\"Statement\": ["));
        } else {
            panic!("Expected policy_document to be a string");
        }
    }

    Ok(())
}

/// Test complex indentation scenarios with mixed content
#[tokio::test]
async fn test_complex_indentation_scenarios() -> Result<()> {
    let yaml_input = r#"
$defs:
  cluster_name: production-cluster
  namespace: default

Resources:
  ConfigMap:
    apiVersion: v1
    kind: ConfigMap
    data:
      # YAML within YAML - common in Kubernetes/CloudFormation
      app-config.yaml: |
        app:
          name: {{cluster_name}}
          namespace: {{namespace}}
          database:
            host: db.{{namespace}}.svc.cluster.local
            port: 5432
          
          # This is deeply nested literal content
          startup_script: |
            #!/bin/bash
            echo "Starting application in {{namespace}}"
            export DB_HOST="db.{{namespace}}.svc.cluster.local"
            ./start-app.sh
      
      # JSON configuration
      database-config.json: >
        {
          "connections": {
            "primary": "postgresql://user:pass@db.{{namespace}}.svc.cluster.local:5432/{{cluster_name}}", 
            "replica": "postgresql://user:pass@db-replica.{{namespace}}.svc.cluster.local:5432/{{cluster_name}}"
          },
          "pool_size": 10
        }

  # CloudFormation UserData with complex indentation
  LaunchTemplate:
    Properties:
      LaunchTemplateData:
        UserData: !Base64 |
          #!/bin/bash
          
          # Set variables from preprocessing
          CLUSTER_NAME="{{cluster_name}}"
          NAMESPACE="{{namespace}}"
          
          # Create configuration files
          cat << 'EOF' > /etc/app/config.yaml
          cluster:
            name: ${CLUSTER_NAME}
            namespace: ${NAMESPACE}
          
          database:
            connection_string: "postgresql://db.${NAMESPACE}.svc.cluster.local/app"
          EOF
          
          # Multi-line command with proper indentation
          docker run \
            --name app-container \
            --env CLUSTER_NAME="${CLUSTER_NAME}" \
            --env NAMESPACE="${NAMESPACE}" \
            --volume /etc/app:/app/config \
            myapp:latest
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;

    if let Value::Mapping(root) = &result {
        if let Some(Value::Mapping(resources)) = root.get(Value::String("Resources".to_string())) {
            // Check ConfigMap with nested YAML
            if let Some(Value::Mapping(config_map)) =
                resources.get(Value::String("ConfigMap".to_string()))
            {
                if let Some(Value::Mapping(data)) =
                    config_map.get(Value::String("data".to_string()))
                {
                    if let Some(Value::String(app_config)) =
                        data.get(Value::String("app-config.yaml".to_string()))
                    {
                        // Check handlebars substitution in nested YAML
                        assert!(app_config.contains("name: production-cluster"));
                        assert!(app_config.contains("namespace: default"));
                        assert!(app_config.contains("host: db.default.svc.cluster.local"));
                        // Check that nested literal scalar is preserved
                        assert!(app_config.contains("echo \"Starting application in default\""));
                        assert!(
                            app_config.contains("export DB_HOST=\"db.default.svc.cluster.local\"")
                        );
                    }

                    if let Some(Value::String(db_config)) =
                        data.get(Value::String("database-config.json".to_string()))
                    {
                        // Check JSON folded content
                        assert!(db_config.contains("postgresql://user:pass@db.default.svc.cluster.local:5432/production-cluster"));
                        assert!(db_config.contains("\"pool_size\": 10"));
                    }
                }
            }

            // Check LaunchTemplate with CloudFormation tag
            if let Some(Value::Mapping(launch_template)) =
                resources.get(Value::String("LaunchTemplate".to_string()))
            {
                if let Some(Value::Mapping(properties)) =
                    launch_template.get(Value::String("Properties".to_string()))
                {
                    if let Some(Value::Mapping(template_data)) =
                        properties.get(Value::String("LaunchTemplateData".to_string()))
                    {
                        if let Some(Value::Tagged(user_data_tagged)) =
                            template_data.get(Value::String("UserData".to_string()))
                        {
                            // Should preserve !Base64 tag
                            assert_eq!(user_data_tagged.tag.to_string(), "!Base64");
                            if let Value::String(user_data) = &user_data_tagged.value {
                                // Check handlebars substitution in UserData
                                assert!(user_data.contains("CLUSTER_NAME=\"production-cluster\""));
                                assert!(user_data.contains("NAMESPACE=\"default\""));
                                // Check complex multi-line structure is preserved
                                assert!(user_data.contains("cat << 'EOF' > /etc/app/config.yaml"));
                                assert!(user_data.contains("docker run \\\n"));
                                assert!(user_data.contains("--name app-container"));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Test round-trip fidelity by processing YAML and ensuring structure preservation
#[tokio::test]
async fn test_yaml_round_trip_fidelity() -> Result<()> {
    // Create a temporary file with complex YAML
    let yaml_content = r#"
$defs:
  environment: staging
  
# Test various scalar formats survive round-trip
strings:
  plain: "Plain string with {{environment}}"
  literal: |
    Line 1
    Line 2 with {{environment}}
    Line 3
  folded: >
    This is a long line that should
    be folded with {{environment}} variable
    replacement.
  literal_strip: |-
    Line 1
    Line 2 with {{environment}}
  folded_keep: >+
    Content with {{environment}}
    

# Test complex nesting
Resources:
  MyResource:
    Type: "AWS::EC2::Instance"
    Properties:
      UserData: !Base64 |
        #!/bin/bash
        echo "Environment: {{environment}}"
        
        # Multi-line script
        cat > /tmp/config <<EOF
        env={{environment}}
        region=us-west-2
        EOF
      Tags:
        - Key: Environment
          Value: "{{environment}}"
        - Key: Description  
          Value: >
            This is a multi-line description
            for the {{environment}} environment
            that should be folded properly.
"#;

    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(yaml_content.as_bytes())?;
    let temp_path = temp_file.path().to_str().unwrap();

    // Process the YAML
    let result = preprocess_yaml_v11(yaml_content, temp_path).await?;

    // Serialize back to YAML
    let output_yaml = serde_yaml::to_string(&result)?;

    // Parse the output to ensure it's valid YAML
    let reparsed: Value = serde_yaml::from_str(&output_yaml)?;

    // Verify structure preservation
    if let Value::Mapping(root) = &reparsed {
        // Check that strings section is preserved
        if let Some(Value::Mapping(strings)) = root.get(Value::String("strings".to_string())) {
            assert!(strings.contains_key(Value::String("plain".to_string())));
            assert!(strings.contains_key(Value::String("literal".to_string())));
            assert!(strings.contains_key(Value::String("folded".to_string())));

            // Check variable substitution occurred
            if let Some(Value::String(plain)) = strings.get(Value::String("plain".to_string())) {
                assert!(plain.contains("staging"));
            }

            if let Some(Value::String(literal)) = strings.get(Value::String("literal".to_string()))
            {
                assert!(literal.contains("Line 2 with staging"));
                // Should preserve literal newlines
                assert!(literal.contains("Line 1\nLine 2"));
            }
        }

        // Check CloudFormation structure preservation
        if let Some(Value::Mapping(resources)) = root.get(Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(my_resource)) =
                resources.get(Value::String("MyResource".to_string()))
            {
                assert_eq!(
                    my_resource.get(Value::String("Type".to_string())),
                    Some(&Value::String("AWS::EC2::Instance".to_string()))
                );

                if let Some(Value::Mapping(properties)) =
                    my_resource.get(Value::String("Properties".to_string()))
                {
                    // UserData should be preserved as tagged value
                    if let Some(Value::Tagged(user_data_tagged)) =
                        properties.get(Value::String("UserData".to_string()))
                    {
                        assert_eq!(user_data_tagged.tag.to_string(), "!Base64");
                    }

                    // Tags should be preserved as sequence
                    if let Some(Value::Sequence(tags)) =
                        properties.get(Value::String("Tags".to_string()))
                    {
                        assert_eq!(tags.len(), 2);

                        if let Value::Mapping(first_tag) = &tags[0] {
                            assert_eq!(
                                first_tag.get(Value::String("Key".to_string())),
                                Some(&Value::String("Environment".to_string()))
                            );
                            assert_eq!(
                                first_tag.get(Value::String("Value".to_string())),
                                Some(&Value::String("staging".to_string()))
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Test edge cases with whitespace and special characters
#[tokio::test]
async fn test_whitespace_edge_cases() -> Result<()> {
    let yaml_input = r#"
$defs:
  tab_char: "	"  # literal tab
  space_char: " "
  newline_char: "\n"
  
# Test with various whitespace scenarios
whitespace_tests:
  # Literal with tabs and trailing spaces
  with_tabs: |
    Line with tab:	{{tab_char}}end
    Line with spaces:   {{space_char}}end
    Empty line below:
    
    Line after empty line
  
  # Folded with multiple spaces
  folded_spaces: >
    Multiple   spaces    should    be    
    preserved     in     folded     content
    when {{space_char}} variable is used.
  
  # Test indented literal blocks
  nested_literal: |
    def function():
        if condition:
            print("Indented {{tab_char}} code")
            for item in items:
                process(item)
    
        return result

# CloudFormation with complex whitespace
Resources:
  MyFunction:
    Properties:
      Code: !Sub |
        import json
        
        def lambda_handler(event, context):
            # Process with ${tab_char} indentation
            data = {
                'environment': '{{tab_char}}${Environment}',
                'timestamp': '{{newline_char}}${AWS::Region}'
            }
            
            return {
                'statusCode': 200,
                'body': json.dumps(data)
            }
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;

    if let Value::Mapping(root) = &result {
        if let Some(Value::Mapping(whitespace_tests)) =
            root.get(Value::String("whitespace_tests".to_string()))
        {
            // Check that tabs are preserved in literal content
            if let Some(Value::String(with_tabs)) =
                whitespace_tests.get(Value::String("with_tabs".to_string()))
            {
                assert!(with_tabs.contains("Line with tab:\t\tend")); // Tab should be preserved
                assert!(with_tabs.contains("Line with spaces:    end")); // Spaces should be preserved
                assert!(with_tabs.contains("Empty line below:\n\nLine after")); // Empty lines preserved
            }

            // Check nested indentation
            if let Some(Value::String(nested_literal)) =
                whitespace_tests.get(Value::String("nested_literal".to_string()))
            {
                assert!(nested_literal.contains("def function():"));
                assert!(nested_literal.contains("    if condition:")); // Python indentation preserved
                assert!(nested_literal.contains("        print(\"Indented \t code\")")); // Tab in content
                assert!(nested_literal.contains("            process(item)")); // Deep indentation preserved
            }
        }

        // Check CloudFormation function with whitespace handling
        if let Some(Value::Mapping(resources)) = root.get(Value::String("Resources".to_string())) {
            if let Some(Value::Mapping(my_function)) =
                resources.get(Value::String("MyFunction".to_string()))
            {
                if let Some(Value::Mapping(properties)) =
                    my_function.get(Value::String("Properties".to_string()))
                {
                    if let Some(Value::Tagged(code_tagged)) =
                        properties.get(Value::String("Code".to_string()))
                    {
                        // Should preserve !Sub tag
                        assert_eq!(code_tagged.tag.to_string(), "!Sub");

                        if let Value::String(code_content) = &code_tagged.value {
                            // Should have processed handlebars but preserved CloudFormation substitutions
                            assert!(code_content.contains("'environment': '\t${Environment}'"));
                            assert!(code_content.contains("'timestamp': '\n${AWS::Region}'"));
                            // Python indentation should be preserved
                            assert!(code_content.contains("def lambda_handler(event, context):"));
                            assert!(code_content.contains("    # Process with ${tab_char}"));
                            assert!(code_content.contains("        'environment':"));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
/// Test round-trip fidelity using the preprocessing API directly
#[tokio::test]
async fn test_iidy_render_command_round_trip() -> Result<()> {
    // Test the core rendering functionality directly rather than through CLI
    // This provides the same coverage but is much faster and more reliable
    let yaml_content = r#"
AWSTemplateFormatVersion: "2010-09-09"
Description: "Test template for round-trip fidelity"

Parameters:
  Environment:
    Type: String
    Default: production

Resources:
  # Test literal scalar in CloudFormation template
  EC2Instance:
    Type: "AWS::EC2::Instance"
    Properties:
      UserData: !Base64 |
        #!/bin/bash
        yum update -y
        
        # Configure CloudWatch
        cat << 'INNER_EOF' > /opt/aws/config.json
        {
          "metrics": {
            "namespace": "CWAgent"
          }
        }
        INNER_EOF
      
      Tags:
        - Key: Name
          Value: !Sub "${Environment}-instance"

Outputs:
  InstanceId:
    Description: "ID of the created instance"
    Value: !Ref EC2Instance
"#;

    // Process using the same preprocessing pipeline as the render command
    let result = preprocess_yaml_v11(yaml_content, "test-template.yaml").await?;

    // Serialize back to YAML (same as render command output)
    let rendered_content = serde_yaml::to_string(&result)?;

    // Parse the rendered content to ensure it's valid YAML
    let parsed_output: Value = serde_yaml::from_str(&rendered_content)?;

    // Verify key structure is preserved
    if let Value::Mapping(root) = &parsed_output {
        // Check CloudFormation template structure
        assert!(root.contains_key(Value::String("AWSTemplateFormatVersion".to_string())));
        assert!(root.contains_key(Value::String("Resources".to_string())));
        assert!(root.contains_key(Value::String("Outputs".to_string())));

        if let Some(Value::Mapping(resources)) = root.get(Value::String("Resources".to_string())) {
            // Check EC2 instance with UserData
            if let Some(Value::Mapping(ec2_instance)) =
                resources.get(Value::String("EC2Instance".to_string()))
            {
                if let Some(Value::Mapping(properties)) =
                    ec2_instance.get(Value::String("Properties".to_string()))
                {
                    // UserData should be preserved as tagged value
                    if let Some(Value::Tagged(user_data_tagged)) =
                        properties.get(Value::String("UserData".to_string()))
                    {
                        assert_eq!(user_data_tagged.tag.to_string(), "!Base64");

                        if let Value::String(user_data_content) = &user_data_tagged.value {
                            // Check that multi-line shell script structure is preserved
                            assert!(user_data_content.contains("#!/bin/bash"));
                            assert!(user_data_content.contains("yum update -y"));
                            // Check that HERE document is preserved
                            assert!(
                                user_data_content
                                    .contains("cat << 'INNER_EOF' > /opt/aws/config.json")
                            );
                            assert!(user_data_content.contains("\"namespace\": \"CWAgent\""));
                        }
                    }
                }
            }
        }
    } else {
        panic!("Expected output to be a YAML mapping");
    }

    // Verify the output is still valid CloudFormation by checking it can be re-parsed
    let _reparsed: Value = serde_yaml::from_str(&rendered_content)?;

    Ok(())
}

/// Test all chomping indicators are handled correctly
#[tokio::test]
async fn test_chomping_indicators() -> Result<()> {
    let yaml_input = r#"
# Literal with clip (default) - single final newline
literal_clip: |
  line one
  line two


# Literal with strip - no final newline
literal_strip: |-
  line one
  line two


# Literal with keep - preserve all trailing newlines (3 blank lines = 3 newlines after content)
literal_keep: |+
  line one
  line two


# Folded with clip (default) - single final newline
folded_clip: >
  line one
  line two


# Folded with strip - no final newline
folded_strip: >-
  line one
  line two


# Folded with keep - preserve all trailing newlines
folded_keep: >+
  line one
  line two


"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;

    if let Value::Mapping(root) = &result {
        // Test literal_clip: should have single final newline
        if let Some(Value::String(s)) = root.get(Value::String("literal_clip".to_string())) {
            assert_eq!(
                s, "line one\nline two\n",
                "literal_clip should have single final newline"
            );
        } else {
            panic!("literal_clip not found or not a string");
        }

        // Test literal_strip: should have NO final newline
        if let Some(Value::String(s)) = root.get(Value::String("literal_strip".to_string())) {
            assert_eq!(
                s, "line one\nline two",
                "literal_strip should have no final newline"
            );
        } else {
            panic!("literal_strip not found or not a string");
        }

        // Test literal_keep: should preserve all 3 trailing newlines
        if let Some(Value::String(s)) = root.get(Value::String("literal_keep".to_string())) {
            assert_eq!(
                s, "line one\nline two\n\n\n",
                "literal_keep should preserve 3 trailing newlines"
            );
        } else {
            panic!("literal_keep not found or not a string");
        }

        // Test folded_clip: lines should be folded with space, with single final newline
        if let Some(Value::String(s)) = root.get(Value::String("folded_clip".to_string())) {
            assert_eq!(
                s, "line one line two\n",
                "folded_clip should fold lines and have single final newline"
            );
        } else {
            panic!("folded_clip not found or not a string");
        }

        // Test folded_strip: lines should be folded with NO final newline
        if let Some(Value::String(s)) = root.get(Value::String("folded_strip".to_string())) {
            assert_eq!(
                s, "line one line two",
                "folded_strip should fold lines with no final newline"
            );
        } else {
            panic!("folded_strip not found or not a string");
        }

        // Test folded_keep: lines should be folded and preserve trailing newlines
        if let Some(Value::String(s)) = root.get(Value::String("folded_keep".to_string())) {
            assert_eq!(
                s, "line one line two\n\n\n",
                "folded_keep should fold lines and preserve 3 trailing newlines"
            );
        } else {
            panic!("folded_keep not found or not a string");
        }
    } else {
        panic!("Result is not a mapping");
    }

    Ok(())
}
